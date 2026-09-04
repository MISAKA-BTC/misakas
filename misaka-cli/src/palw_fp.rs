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
pub async fn submit(
    ctx: &Ctx,
    path: &Path,
    yes: bool,
    material_out: Option<&Path>,
    capture: Option<&Path>,
    dsl_payload: Option<&Path>,
) -> Result<(), CliError> {
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
            let (bytes, answer_bytes): (Vec<u8>, Option<Vec<u8>>) = match capture {
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
                    // **The answer envelope beside the material** (ADR-0084 Decision 5): the
                    // answer's ids, read off the capture by the family decoder, so the node serves
                    // a few kilobytes to a seat when the capture itself would not fit the
                    // transport. Refused by name when the capture's answer is not this
                    // commitment's (its count is not `decode_tokens_executed`).
                    let ids = misaka_palw_base0::produce::base0_material_decode_any_v1(&capture_bytes)
                        .map(|retention| retention.generated_token_ids().to_vec())
                        .map_err(|e| CliError::new(exit::GENERIC, format!("{}: the capture does not decode: {e:?}", capture_path.display())))?;
                    if ids.len() != payload.commitment.decode_tokens_executed as usize {
                        return Err(CliError::new(
                            exit::GENERIC,
                            format!(
                                "{} holds an answer of {} ids and this commitment executed {} decode tokens — not this claim's capture",
                                capture_path.display(),
                                ids.len(),
                                payload.commitment.decode_tokens_executed
                            ),
                        ));
                    }
                    let answer = kaspa_consensus_core::palw_freeprompt_v3::palw_fp_answer_encode_v1(
                        &payload.commitment.job,
                        &payload.prompt_token_ids,
                        &ids,
                    );
                    (
                        kaspa_consensus_core::palw_freeprompt_v3::palw_fp_capture_encode_v1(
                            &payload.commitment.job,
                            &payload.prompt_token_ids,
                            &capture_bytes,
                        ),
                        Some(answer),
                    )
                }
                None => (
                    kaspa_consensus_core::palw_freeprompt_v3::palw_fp_material_encode_v1(
                        &payload.commitment.job,
                        &payload.prompt_token_ids,
                    ),
                    None,
                ),
            };
            std::fs::create_dir_all(dir).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", dir.display())))?;
            let file = dir.join(format!("{claim}.material"));
            let partial = file.with_extension("material.partial");
            std::fs::write(&partial, &bytes).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", partial.display())))?;
            let answer = match answer_bytes {
                Some(answer) => {
                    // `.answer` beside `.material`: the suffix the node's resolver reads
                    // (`palw_retained_answer_path`) and `misaka-palw-fp-submit::ANSWER_SUFFIX` spell.
                    let answer_file = dir.join(format!("{claim}.answer"));
                    let answer_partial = answer_file.with_extension("answer.partial");
                    std::fs::write(&answer_partial, &answer)
                        .map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", answer_partial.display())))?;
                    Some((answer_partial, answer_file))
                }
                None => None,
            };
            // ADR-0078 Decision 6: the DSL under the data-availability election. Staged the same
            // way, beside the material, only when the operator passed the gateway's `FPD1`
            // payload — the default is off, and "off" means no file for the node to serve.
            let dsl = match dsl_payload {
                Some(dsl_path) => {
                    let dsl_bytes =
                        std::fs::read(dsl_path).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", dsl_path.display())))?;
                    let decoded = kaspa_consensus_core::palw_derived_v1::palw_fp_dsl_decode_v1(&dsl_bytes)
                        .ok_or_else(|| CliError::new(exit::GENERIC, format!("{} is not an FPD1 DSL payload", dsl_path.display())))?;
                    if decoded.claim_id != claim {
                        return Err(CliError::new(
                            exit::GENERIC,
                            format!("the DSL payload names claim {} but this transaction commits claim {claim}", decoded.claim_id),
                        ));
                    }
                    let dsl_file = dir.join(format!("{claim}.dsl"));
                    let dsl_partial = dsl_file.with_extension("dsl.partial");
                    std::fs::write(&dsl_partial, &dsl_bytes)
                        .map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", dsl_partial.display())))?;
                    Some((dsl_partial, dsl_file))
                }
                None => None,
            };
            Some((partial, file, answer, dsl))
        }
        None => {
            if dsl_payload.is_some() {
                return Err(CliError::new(
                    exit::GENERIC,
                    "--dsl-payload needs --material-out (the node's retention directory)".to_string(),
                ));
            }
            None
        }
    };

    let nv = connect(ctx).await?;
    let submitted = nv.client.submit_transaction(RpcTransaction::from(&tx), false).await.map_err(|e| {
        // The staged file is not a claim's material if the claim never reached the chain.
        if let Some((partial, _, answer, dsl)) = &staged {
            let _ = std::fs::remove_file(partial);
            if let Some((answer_partial, _)) = answer {
                let _ = std::fs::remove_file(answer_partial);
            }
            if let Some((dsl_partial, _)) = dsl {
                let _ = std::fs::remove_file(dsl_partial);
            }
        }
        CliError::new(exit::GENERIC, format!("submit {txid}: {e}"))
    })?;

    // Accepted: the obligation is real, so the file takes its real name.
    let mut material_note: Option<String> = None;
    let mut answer_note: Option<String> = None;
    let mut dsl_note: Option<String> = None;
    if let Some((partial, file, answer, dsl)) = staged {
        std::fs::rename(&partial, &file).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", file.display())))?;
        material_note = Some(file.display().to_string());
        if let Some((answer_partial, answer_file)) = answer {
            std::fs::rename(&answer_partial, &answer_file)
                .map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", answer_file.display())))?;
            answer_note = Some(answer_file.display().to_string());
        }
        if let Some((dsl_partial, dsl_file)) = dsl {
            std::fs::rename(&dsl_partial, &dsl_file)
                .map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", dsl_file.display())))?;
            dsl_note = Some(dsl_file.display().to_string());
        }
    }

    match ctx.output {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::json!({ "submitted": true, "txid": submitted.to_string(), "material": material_note, "answer": answer_note, "dsl": dsl_note })
            )
        }
        _ => {
            println!("submitted {submitted}");
            if let Some(file) = &material_note {
                println!("  DA material written: {file}");
            }
            if let Some(file) = &answer_note {
                println!("  answer envelope beside it (served when the capture is over the transport cap, ADR-0084): {file}");
            }
            if let Some(file) = &dsl_note {
                println!("  DSL under the DA election (ADR-0078 Decision 6): {file}");
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

/// **`palw submit-object`** (ADR-0075): carry lifecycle objects the chain judges on its own — a
/// `FamilyCertified` whose evidence the court grades in the transition, the `ObjectChunk`s of one
/// too large for a carrier (Decision 14), or a `ClassLaneCertified` checked against the class's
/// own profile hash and the chain's certified families. The carriers are funded from the CLI
/// key's largest mature UTXO, each after the first from the previous carrier's change, so a
/// chunked object goes out as one chained burst; every fee is sized from its carrier's own
/// compute mass, because a drill's chunk is a hundred kilobytes and a send-sized fee would be
/// refused by the mempool as insufficient.
pub async fn submit_objects(ctx: &Ctx, ks: &crate::keys::KeySource, paths: &[std::path::PathBuf], yes: bool) -> Result<(), CliError> {
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
    use kaspa_consensus_core::tx::UtxoEntry;

    let mut objects = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = std::fs::read(path).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", path.display())))?;
        let object: PalwConsensusObjectV2 = borsh::from_slice(&bytes)
            .map_err(|e| CliError::new(exit::GENERIC, format!("{} is not a borsh consensus object: {e}", path.display())))?;
        kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object)
            .map_err(|why| CliError::new(exit::GENERIC, format!("{}: {why}", path.display())))?;
        let summary = match &object {
            PalwConsensusObjectV2::FamilyCertified { evidence } => {
                // Graded here before a fee is spent: the chain would refuse the same bytes, and
                // a refused object is a dropped carrier — the fee gone, nothing recorded.
                let family = evidence.grade().map_err(|e| {
                    CliError::new(exit::GENERIC, format!("this build's court refuses the evidence, so the chain would too: {e}"))
                })?;
                if bytes.len() > kaspa_consensus_core::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES {
                    return Err(CliError::new(
                        exit::GENERIC,
                        format!(
                            "{} is {} bytes, above one carrier's {}; submit the `.chunkN` files palw-certify wrote instead",
                            path.display(),
                            bytes.len(),
                            kaspa_consensus_core::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES
                        ),
                    ));
                }
                format!(
                    "FamilyCertified: {} lane, family {} (digest {}), {} fault vectors, {} kernels",
                    evidence.lane(),
                    family.family_id,
                    family.digest(),
                    evidence.vector_count(),
                    family.kernel_ids.len()
                )
            }
            PalwConsensusObjectV2::ClassLaneCertified { class_id, lane, profile } => {
                let derived = profile.shape_profile_id();
                if derived != *class_id {
                    return Err(CliError::new(
                        exit::GENERIC,
                        format!("the object names class {class_id} but its profile hashes to {derived}; the chain would refuse it"),
                    ));
                }
                format!("ClassLaneCertified: {lane} lane, class {class_id}")
            }
            PalwConsensusObjectV2::ObjectChunk { group, index, count, bytes: part } => {
                format!("ObjectChunk: part {index} of {count} of group {group}, {} bytes", part.len())
            }
            // ADR-0078: a derivation. Checked here for shape only, exactly as the ride list will;
            // whether its claim exists, its output root is the claim's and its key is the bond's
            // is the chain's to say.
            PalwConsensusObjectV2::DerivedArtifactV1 { object, signature } => {
                use kaspa_consensus_core::palw_derived_v1::{derived_id_v1, kind};
                format!(
                    "DerivedArtifactV1: claim {}, kind {} ({}), transformer {}, artifact {} bytes, derived id {}, signature {} bytes",
                    object.claim_id,
                    object.kind,
                    kind::name(object.kind).unwrap_or("unassigned"),
                    object.transformer_id,
                    object.artifact_bytes,
                    derived_id_v1(object),
                    signature.len()
                )
            }
            other => format!("{other:?}"),
        };
        objects.push((path.clone(), object, summary));
    }

    let nv = connect(ctx).await?;
    let key = ks.load_key()?;
    let addr = key.funding_address(nv.params.prefix());
    let candidates = spendable_candidates_v1(&nv, &addr).await?;
    let (first_outpoint, first_entry) = candidates
        .first()
        .cloned()
        .ok_or_else(|| CliError::new(exit::GENERIC, format!("no mature, unbonded, unspent UTXO at {addr} to fund the carrier")))?;

    // Build every carrier first — chained on the previous one's change — so a refusal costs nothing.
    let mut funding_outpoint = first_outpoint;
    let mut funding_entry = first_entry;
    let mut carriers = Vec::with_capacity(objects.len());
    for (path, object, summary) in &objects {
        let (tx, compute_mass, fee) = build_carrier_priced_v1(&key, &nv, object, funding_outpoint, &funding_entry)
            .map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", path.display())))?;
        let change = tx.outputs.first().ok_or_else(|| CliError::new(exit::GENERIC, "the carrier has no change output".to_string()))?;
        funding_outpoint = kaspa_consensus_core::tx::TransactionOutpoint::new(tx.id(), 0);
        funding_entry = UtxoEntry::new(change.value, change.script_public_key.clone(), 0, false);
        carriers.push((path.clone(), summary.clone(), tx, compute_mass, fee));
    }

    if !yes {
        match ctx.output {
            OutputFormat::Json => println!(
                "{}",
                serde_json::json!({
                    "dry_run": true,
                    "carriers": carriers.iter().map(|(path, summary, tx, mass, fee)| serde_json::json!({
                        "object": path.display().to_string(), "summary": summary, "txid": tx.id().to_string(),
                        "payload_bytes": tx.payload.len(), "compute_mass": mass, "fee_sompi": fee,
                    })).collect::<Vec<_>>(),
                    "funding": format!("{}:{}", first_outpoint.transaction_id, first_outpoint.index),
                })
            ),
            _ => {
                for (path, summary, tx, mass, fee) in &carriers {
                    println!("{summary}");
                    println!(
                        "  carrier {} for {} ({}-byte payload, compute mass {mass}, fee {fee} sompi)",
                        tx.id(),
                        path.display(),
                        tx.payload.len()
                    );
                }
                println!(
                    "funded from {}:{}, each carrier from the previous one's change",
                    first_outpoint.transaction_id, first_outpoint.index
                );
                println!("dry run — nothing was sent. Re-run with --yes to submit.");
            }
        }
        return Ok(());
    }
    let mut submitted = Vec::with_capacity(carriers.len());
    for (path, summary, tx, _, fee) in &carriers {
        // A funding UTXO the mempool already holds a spend of (an earlier burst's change not yet
        // in a block) is not a failure of the object: say what to do instead of a bare reject.
        nv.client.submit_transaction(tx.as_ref().into(), false).await.map_err(|e| {
            let text = e.to_string();
            if text.contains("already spent") {
                CliError::new(
                    exit::GENERIC,
                    format!(
                        "submit the carrier for {}: the funding UTXO is spent by a transaction still in the mempool (an earlier \
                         submit-object's change has not been mined yet) — wait for a block and re-run; nothing was carried: {text}",
                        path.display()
                    ),
                )
            } else {
                CliError::new(exit::GENERIC, format!("submit the carrier for {}: {text}", path.display()))
            }
        })?;
        match ctx.output {
            OutputFormat::Json => {}
            _ => println!("submitted {} — {summary} (fee {fee} sompi)", tx.id()),
        }
        submitted.push(serde_json::json!({ "object": path.display().to_string(), "txid": tx.id().to_string(), "fee_sompi": fee }));
    }
    match ctx.output {
        OutputFormat::Json => println!("{}", serde_json::json!({ "ok": true, "submitted": true, "carriers": submitted })),
        _ => println!("the chain grades them when the carriers are accepted; a chunked object applies in the block that completes it"),
    }
    Ok(())
}

// =================================================================================================
// The carriage path, shared — one funding rule and one fee rule for every lifecycle carrier
// =================================================================================================

/// **Every outpoint this key may spend, mempool included, largest first.**
///
/// Lifted out of `submit_objects` unchanged so `palw court-close` funds a carrier the SAME way:
/// two callers picking funding by two rules is how "output already spent by transaction in the
/// mempool" came back the last time, and the reasoning below is the reason it does not.
///
/// **Ask the mempool before choosing funding.** An earlier burst's carriers spend a UTXO the
/// index still lists and pay change the index does not list yet; picking by the index alone is
/// "output already spent by transaction in the mempool", every second run. Our own pending
/// transactions are read back, their inputs excluded, their change to us made eligible.
///
/// The pending change is also what `palw court-close` resumes from: a carrier the node is still
/// holding is a carrier that was sent, and its change is the funding of the next part.
pub(crate) async fn spendable_candidates_v1(
    nv: &crate::wallet::NodeView,
    addr: &kaspa_addresses::Address,
) -> Result<Vec<(kaspa_consensus_core::tx::TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry)>, CliError> {
    use kaspa_consensus_core::tx::UtxoEntry;
    let all = crate::wallet::page_all(nv, addr).await?;
    let mut pending_spent: std::collections::HashSet<kaspa_consensus_core::tx::TransactionOutpoint> = Default::default();
    let mut pending_change: Vec<(kaspa_consensus_core::tx::TransactionOutpoint, UtxoEntry)> = Vec::new();
    if let Ok(entries) = nv.client.get_mempool_entries_by_addresses(vec![addr.clone()], false, false).await {
        use kaspa_consensus_core::tx::{TransactionInput, TransactionOutpoint, TransactionOutput};
        let my_spk = kaspa_txscript::pay_to_address_script(addr);
        let mut seen: std::collections::BTreeSet<kaspa_consensus_core::tx::TransactionId> = Default::default();
        for by_address in entries {
            for entry in by_address.sending.iter().chain(by_address.receiving.iter()) {
                let rtx = &entry.transaction;
                let inputs: Vec<TransactionInput> = rtx
                    .inputs
                    .iter()
                    .map(|i| {
                        TransactionInput::new(
                            TransactionOutpoint::new(i.previous_outpoint.transaction_id, i.previous_outpoint.index),
                            i.signature_script.clone(),
                            i.sequence,
                            i.sig_op_count,
                        )
                    })
                    .collect();
                let outputs: Vec<TransactionOutput> =
                    rtx.outputs.iter().map(|o| TransactionOutput::new(o.value, o.script_public_key.clone())).collect();
                let tx = Transaction::new(
                    rtx.version,
                    inputs,
                    outputs,
                    rtx.lock_time,
                    rtx.subnetwork_id.clone(),
                    rtx.gas,
                    rtx.payload.clone(),
                );
                let id = tx.id();
                if !seen.insert(id) {
                    continue;
                }
                for input in &tx.inputs {
                    pending_spent.insert(input.previous_outpoint);
                }
                for (index, output) in tx.outputs.iter().enumerate() {
                    if output.script_public_key == my_spk {
                        pending_change.push((
                            TransactionOutpoint::new(id, index as u32),
                            UtxoEntry::new(output.value, output.script_public_key.clone(), 0, false),
                        ));
                    }
                }
            }
        }
    }
    let mut candidates: Vec<(kaspa_consensus_core::tx::TransactionOutpoint, UtxoEntry)> = all
        .iter()
        .filter(|u| u.mature && !u.bonded && !pending_spent.contains(&u.outpoint))
        .map(|u| (u.outpoint, u.entry.clone()))
        .collect();
    candidates.extend(pending_change.into_iter().filter(|(outpoint, _)| !pending_spent.contains(outpoint)));
    candidates.sort_by(|a, b| b.1.amount.cmp(&a.1.amount));
    Ok(candidates)
}

/// **What the chain may charge this carrier as RENT rather than as carriage** (ADR-0075 SA-1/SA-2).
///
/// One spelling, read by every carrier builder. Paid on every network, armed fence or not: a
/// carrier the chain drops for underpaying is a fee spent and nothing recorded.
///
/// The room a chunk group reserves is charged to chunk 0 alone, which is the chunk that opens the
/// group: the carriers are CHAINED on one another's change, so no later one can be mined first.
/// The chunk that COMPLETES the group carries the object into its block, so it owes the grading
/// rent — and a single chunk cannot say how many vectors the assembled object holds, so it pays
/// for the most the rules allow. `index` and `count` come off a FILE an operator hands the tool,
/// so the comparison is widened to `u16`: `255 + 1` on a `u8` is a panic rather than a refusal.
pub(crate) fn carrier_rent_v1(object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2) -> u64 {
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
    match object {
        Obj::FamilyCertified { evidence } => {
            kaspa_consensus_core::palw_state_v2::palw_certification_min_fee_v1(evidence.vector_count())
        }
        Obj::ObjectChunk { index, count, .. } => {
            let opener = if *index == 0 { kaspa_consensus_core::palw_state_v2::palw_object_chunk_group_rent_v1() } else { 0 };
            let completing = if u16::from(*index) + 1 == u16::from(*count) {
                kaspa_consensus_core::palw_state_v2::palw_certification_min_fee_v1(
                    kaspa_consensus_core::palw_state_v2::PALW_CERTIFICATION_MAX_VECTORS,
                )
            } else {
                0
            };
            opener.max(completing)
        }
        _ => 0,
    }
}

/// **Build one lifecycle carrier and price it from its own mass** — the fee rule, once.
///
/// Size the fee from the carrier itself: build once at the floor, read its mass, rebuild. A
/// send-sized flat fee is refused by the mempool as insufficient on a hundred-kilobyte chunk, and
/// a flat default has already cost this repository a lane.
pub(crate) fn build_carrier_priced_v1(
    key: &kaspa_pq_validator_core::ValidatorKey,
    nv: &crate::wallet::NodeView,
    object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    funding_outpoint: kaspa_consensus_core::tx::TransactionOutpoint,
    funding_entry: &kaspa_consensus_core::tx::UtxoEntry,
) -> Result<(Transaction, u64, u64), String> {
    use kaspa_consensus_core::mass::MassCalculator;
    let floor = kaspa_pq_validator_core::ATTESTATION_TX_FEE_FLOOR_SOMPI;
    let calc = MassCalculator::new(
        nv.params.mass_per_tx_byte,
        nv.params.mass_per_script_pub_key_byte,
        nv.params.mass_per_sig_op,
        nv.params.storage_mass_parameter,
    );
    let probe =
        key.build_palw_lifecycle_tx(object, funding_outpoint, funding_entry, floor).map_err(|e| format!("build the carrier: {e}"))?;
    let compute_mass = calc.calc_non_contextual_masses(&probe).compute_mass;
    let fee = kaspa_pq_validator_core::relay_fee_for_compute_mass(compute_mass).max(floor).max(carrier_rent_v1(object));
    if funding_entry.amount <= fee {
        return Err(format!("the funding holds {} sompi, under its {fee} sompi fee", funding_entry.amount));
    }
    let tx =
        key.build_palw_lifecycle_tx(object, funding_outpoint, funding_entry, fee).map_err(|e| format!("build the carrier: {e}"))?;
    Ok((tx, compute_mass, fee))
}

/// [`build_carrier_priced_v1`] without the mass, for a caller that only reports the fee.
pub(crate) fn build_carrier_v1(
    key: &kaspa_pq_validator_core::ValidatorKey,
    nv: &crate::wallet::NodeView,
    object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    funding_outpoint: kaspa_consensus_core::tx::TransactionOutpoint,
    funding_entry: &kaspa_consensus_core::tx::UtxoEntry,
) -> Result<(Transaction, u64), String> {
    build_carrier_priced_v1(key, nv, object, funding_outpoint, funding_entry).map(|(tx, _, fee)| (tx, fee))
}

// ---------------------------------------------------------------------------------------------
// `misaka palw certified` — the two lane facts a builder must know BEFORE it builds
// ---------------------------------------------------------------------------------------------

/// **Is this class seated on the free-prompt lane, and which decode ruleset is the chain playing?**
///
/// Two reads off one `GetPalwProducerFacts` call, and they are together because a builder needs
/// both before it commits anything and can guess neither:
///
/// * `fp_certified` (ADR-0075) — the genesis certified set ∪ the chain set, the same two sets
///   `FreePromptCommitted` refuses on (`FreePromptLaneUncertified`). A job on an uncertified class
///   is still ANSWERED — the answer is the product — and its commitment is unsubmittable, so a
///   gateway that could not ask this discovered it by losing a fee.
/// * `fp_decode_rules_armed` (ADR-0082 Decisions 10/11) — whether `palw_fp_decode_rules` is in
///   force at the chain point the facts were read at. Past the fence a job carries
///   `(sampling_seed, temperature_q)` inside its context hash and decode leaves are what earn, so
///   a builder on the wrong side of it produces claims that are honest and unreproducible.
///
/// Nothing signs and nothing spends: one read. A node older than the field answers `false`, which
/// is the fail-closed reading the wire format documents — hold back and retry, never submit on an
/// assumption.
pub async fn certified(ctx: &Ctx, class_id: &str, json: bool) -> Result<(), CliError> {
    // The class id is parsed HERE, before a connection is opened, so a typo is a message about the
    // argument rather than a round trip that comes back "this chain does not have that class".
    let class = class_id
        .parse::<kaspa_consensus_core::Hash64>()
        .map_err(|_| CliError::new(exit::GENERIC, format!("class id '{class_id}' is not a 128-hex Hash64")))?;
    let reader = crate::palw_derived::connect(ctx).await?;
    let facts = reader
        .client
        .get_palw_producer_facts(class.to_string(), String::new(), 0, false)
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwProducerFacts: {e}")))?;

    let as_json = json || ctx.output == OutputFormat::Json;
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "schema": "misaka.palw.chain-lane-facts.v1",
                "available": facts.available,
                "class_id": facts.class_id,
                "chain_point": facts.chain_point,
                "daa_score": facts.daa_score,
                "fp_certified": facts.fp_certified,
                "fp_decode_rules_armed": facts.fp_decode_rules_armed,
                "fp_quanta_per_canonical_job": facts.fp_quanta_per_canonical_job,
                "fp_max_quanta_per_receipt": facts.fp_max_quanta_per_receipt,
            }))
            .expect("serializable")
        );
    } else if !facts.available {
        println!("class {class}: this chain does not know it (or is not a ConsensusV2 network)");
    } else {
        println!("class {}", facts.class_id);
        println!("  at chain point {} (daa {})", facts.chain_point, facts.daa_score);
        println!(
            "  free-prompt lane   : {}",
            if facts.fp_certified {
                "CERTIFIED — a commitment on this class enters the state"
            } else {
                "uncertified — FreePromptCommitted would be refused (FreePromptLaneUncertified)"
            }
        );
        println!(
            "  fp decode rules    : {}",
            if facts.fp_decode_rules_armed {
                "ARMED — jobs carry (sampling_seed, temperature_q) and decode leaves earn (ADR-0082 D10/D11)"
            } else {
                "dormant — the pre-ADR-0082 job shape; a job built for the armed rules is unreproducible here"
            }
        );
        println!("  quanta per job / receipt cap: {} / {}", facts.fp_quanta_per_canonical_job, facts.fp_max_quanta_per_receipt);
    }

    // **An unavailable class is a non-zero exit**, the same shape `palw derived` takes for a claim
    // the chain does not hold: a script that pipes this into a decision must not read "the chain
    // never heard of this class" as "uncertified", which is a different fact with a different fix.
    if !facts.available {
        return Err(CliError::new(exit::GENERIC, format!("this chain holds no class {class}")));
    }
    Ok(())
}

#[cfg(test)]
mod rent_tests {
    /// **The rent the chain charges is the relay rate this tool pays** (ADR-0075 SA-1/SA-2).
    ///
    /// `kaspa_consensus_core::palw_state_v2::palw_relay_fee_for_mass_v1` is a MIRROR of
    /// `kaspa_pq_validator_core::relay_fee_for_compute_mass`: the validator-core crate depends on
    /// consensus-core, so the consensus side cannot import the original and the two are one rate
    /// spelled twice. A mirrored rate drifts silently and the drift is invisible until a live
    /// carrier is dropped for underpaying a price its own submitter computed — so it is asserted
    /// here, in the one crate that sees both spellings.
    #[test]
    fn the_rent_the_chain_charges_is_the_relay_rate_this_tool_pays() {
        for mass in [0u64, 1, 999, 1_000, 20_000, 100_000, 800_000, 1 << 20, u64::MAX / 20_000] {
            assert_eq!(
                kaspa_consensus_core::palw_state_v2::palw_relay_fee_for_mass_v1(mass),
                kaspa_pq_validator_core::relay_fee_for_compute_mass(mass),
                "the two spellings of the relay rate disagree at mass {mass}"
            );
        }
    }
}
