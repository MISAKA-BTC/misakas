//! `misaka-palw-rfc1-drill-claim` — the RFC-0001 drill's claim PRODUCER for the floor class (`PALW-BASE-0/rc`), the in-repo integer
//! engine every drill node already resolves. It runs one real job on this machine and writes the gateway's outbox artifacts
//! (`fp-job-<id>.result.borsh`, `.commitment-unsigned.borsh`, and the dense capture `.capture.bin`) that `misaka-palw-fp-rail`
//! signs and submits (`--artifact <outbox>/fp-job-<id> --capture <…>.capture.bin --submit --rpc …`). Nothing here signs or spends.
//!
//! ```text
//! misaka-palw-rfc1-drill-claim --kind constraint|constraint2|prefix|inherit --identity <identity.json> --outbox <dir>
//!     (--rpc <host:port> | --anchor-block <128hex> --anchor-daa <n>) [--emit-token-table <file>] [--network-id <s>]
//! ```
//!
//! * `constraint`  — a version-6 job under a first-form constraint (a JSON string `"abc"` or `"cab"`) over the floor's byte table.
//! * `constraint2` — the same under a second-form constraint (a union with a `$ref`).
//! * `prefix`      — a version-11 job naming a cached prefix state.
//! * `inherit`     — a version-12 job (stage 2b, inherited prefix leaves).
//!
//! A constraint kind also writes the class's token table (`--emit-token-table`, default `<outbox>/floor.palwtokens`): every node that
//! seats the claim must load it (`--palw-token-table`, or beside the class artifact as `<artifact>.palwtokens`), or it abstains.
//! Prints the artifact stem on stdout. Exit 0 on success.

#[path = "../chain.rs"]
#[allow(dead_code)]
mod chain;

use std::path::PathBuf;
use std::sync::Arc;

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_fp_constraint_job_v1::{
    PALW_FP_CONSTRAINT_VERSION, PalwConstraintMaskV1, PalwFpConstraintTailV1, PalwTokenTableV1, palw_fp_with_constraint_scope_v1,
    palw_token_table_file_encode_v1,
};
use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, palw_fp_commitment_v3};
use kaspa_consensus_core::palw_fp_prefix_v1::{PALW_FP_PREFIX_INHERIT_VERSION, PALW_FP_PREFIX_VERSION, palw_fp_prefix_state_root_v1};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFpCommitmentTxPayloadV3, PalwFpJobTailV1,
    PalwFpPrefixStateV1, PalwFpWorkerResultV3, PalwFreePromptJobV3,
};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_hashes::Hash64;
use misaka_palw_base0::backend::Base0Backend;

fn die(why: impl std::fmt::Display) -> ! {
    eprintln!("misaka-palw-rfc1-drill-claim: {why}");
    std::process::exit(1)
}

fn hex64(s: &str, what: &str) -> Hash64 {
    let mut out = [0u8; 64];
    if s.len() != 128 || faster_hex::hex_decode(s.as_bytes(), &mut out).is_err() {
        die(format!("{what} is not 128 hex chars"));
    }
    Hash64::from_bytes(out)
}

fn hex_of(h: Hash64) -> String {
    faster_hex::hex_string(h.as_bytes().as_slice())
}

fn floor_backend(form: PalwPromptIdsFormV1) -> Base0Backend {
    use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
    use kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES;
    use misaka_palw_base0::classes::{canonical_class_by_model_id_v1, resolve_class_v1};
    let court = PalwCourtParamsV2::new(PALW_STEP_MAX_LEAVES, 4, 2).unwrap_or_else(|e| die(format!("the court: {e:?}")));
    let entry = canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").unwrap_or_else(|| die("the floor class is not registered in this build"));
    let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().unwrap_or_else(|e| die(format!("the floor's pinned root: {e:?}")));
    let class = resolve_class_v1(&court, entry.class_id(), root, &[]).unwrap_or_else(|e| die(format!("the floor resolves from nothing: {e:?}")));
    Base0Backend::new(class).with_step_ladder_cap(court.max_step_leaf_count()).with_prompt_ids_form(form)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut kind, mut identity, mut outbox, mut rpc, mut anchor_block, mut anchor_daa, mut table_out, mut network_id) =
        (None, None, None, None, None, None, None, b"misaka-palw-rc".to_vec());
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().cloned().unwrap_or_else(|| die(format!("{name} needs a value")));
        match a.as_str() {
            "--kind" => kind = Some(value("--kind")),
            "--identity" => identity = Some(PathBuf::from(value("--identity"))),
            "--outbox" => outbox = Some(PathBuf::from(value("--outbox"))),
            "--rpc" => rpc = Some(value("--rpc")),
            "--anchor-block" => anchor_block = Some(hex64(&value("--anchor-block"), "--anchor-block")),
            "--anchor-daa" => anchor_daa = Some(value("--anchor-daa").parse::<u64>().unwrap_or_else(|e| die(format!("--anchor-daa: {e}")))),
            "--emit-token-table" => table_out = Some(PathBuf::from(value("--emit-token-table"))),
            "--network-id" => network_id = value("--network-id").into_bytes(),
            other => die(format!(
                "unknown argument {other:?}\nusage: misaka-palw-rfc1-drill-claim --kind constraint|constraint2|prefix|inherit --identity <identity.json> \
                 --outbox <dir> (--rpc <host:port> | --anchor-block <128hex> --anchor-daa <n>) [--emit-token-table <file>] [--network-id <s>]"
            )),
        }
    }
    let kind = kind.unwrap_or_else(|| die("--kind is required"));
    let identity: serde_json::Value = {
        let path = identity.unwrap_or_else(|| die("--identity <identity.json> is required (misaka-palw-fp-rail --print-identity)"));
        serde_json::from_slice(&std::fs::read(&path).unwrap_or_else(|e| die(format!("{}: {e}", path.display())))).unwrap_or_else(|e| die(format!("identity: {e}")))
    };
    let field = |name: &str| identity.get(name).and_then(|v| v.as_str()).unwrap_or_else(|| die(format!("identity.json has no {name}"))).to_string();
    let outbox = outbox.unwrap_or_else(|| die("--outbox is required"));
    std::fs::create_dir_all(&outbox).unwrap_or_else(|e| die(format!("{}: {e}", outbox.display())));

    let network_domain = hex64(&field("network_domain"), "network_domain");
    let class_id = hex64(&field("class_id"), "class_id");
    let bond_txid = hex64(&field("bond_txid"), "bond_txid");
    let bond_index = identity.get("bond_index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let operator_id = hex64(&field("operator_id"), "operator_id");
    let executor_pubkey = {
        let text = field("executor_pubkey");
        let mut out = vec![0u8; text.len() / 2];
        faster_hex::hex_decode(text.as_bytes(), &mut out).unwrap_or_else(|e| die(format!("executor_pubkey: {e}")));
        out
    };

    let (anchor_block, anchor_daa) = match (anchor_block, anchor_daa, rpc) {
        (Some(b), Some(d), _) => (b, d),
        (_, _, Some(endpoint)) => {
            let src = chain::RpcChainSource::new(&endpoint, hex_of(class_id), hex_of(bond_txid), bond_index, 10).unwrap_or_else(|e| die(e));
            let facts = src.read();
            if let Some(why) = facts.read_error {
                die(format!("cannot read the anchor from {endpoint}: {why}"));
            }
            (facts.anchor_block, facts.anchor_daa)
        }
        _ => die("give --rpc <host:port> or both --anchor-block and --anchor-daa"),
    };

    let backend = floor_backend(PalwPromptIdsFormV1::Flat);
    let floor = backend.profile().shape_profile_id();
    if class_id != floor {
        eprintln!("note: identity class {} is not the floor {}; the job names the identity's class", hex_of(class_id), hex_of(floor));
    }
    let vocab = backend.profile().vocab_size;

    let (prompt, version, limit): (Vec<u32>, u16, u32) = match kind.as_str() {
        "constraint" | "constraint2" => (vec![17, 3, 91, 4], PALW_FP_CONSTRAINT_VERSION, 8),
        "prefix" => (vec![17, 3, 911, 44, 5], PALW_FP_PREFIX_VERSION, 6),
        "inherit" => (vec![17, 3, 911, 44, 5], PALW_FP_PREFIX_INHERIT_VERSION, 6),
        other => die(format!("--kind {other:?} is not constraint, constraint2, prefix or inherit")),
    };
    let mut nonce = [0u8; 32];
    for (i, b) in nonce.iter_mut().enumerate() {
        *b = (anchor_daa as u8).wrapping_add(i as u8) ^ kind.len() as u8;
    }
    let mut job = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain,
        class_id,
        executor_bond: kaspa_consensus_core::tx::TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_bytes(bond_txid.as_bytes()), bond_index),
        executor_pubkey,
        operator_id,
        anchor_block,
        anchor_daa,
        job_nonce: nonce,
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &prompt)
            .unwrap_or_else(|e| die(format!("the ids do not commit: {e:?}"))),
        prompt_tokens: prompt.len() as u32,
        decode_token_limit: limit,
        max_context_tokens: backend.profile().n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: [0x3C; 32],
        temperature_q: 1 << 24,
        decode: None,
        tail: None,
    };

    let mut table: Option<Arc<PalwTokenTableV1>> = None;
    match kind.as_str() {
        "constraint" | "constraint2" => {
            use kaspa_consensus_core::palw_decode_select_v2::{PALW_DECODE_SEED_GREEDY, PALW_DECODE_TEMPERATURE_GREEDY};
            let mut t = PalwTokenTableV1 {
                entries: (0..vocab).map(|i| Some(vec![0x20 + (i % 95) as u8])).collect(),
                eog_token_ids: vec![vocab - 1],
                tokenizer_id: Hash64::default(),
            };
            t.tokenizer_id = t.digest();
            let t = Arc::new(t);
            let bytes = if kind == "constraint" {
                let schema = misaka_palw_constraint::schema::parse(&serde_json::json!({ "type": "string", "enum": ["abc", "cab"] }))
                    .unwrap_or_else(|e| die(format!("schema: {e:?}")));
                misaka_palw_constraint::compile::compile_v1(&schema).unwrap_or_else(|e| die(format!("compile: {e:?}"))).to_bytes()
            } else {
                let schema = misaka_palw_constraint::schema::parse_v2(&serde_json::json!({
                    "$defs": { "p": { "const": "p" } },
                    "anyOf": [
                        { "type": "object", "properties": { "kind": { "$ref": "#/$defs/p" } }, "required": ["kind"], "additionalProperties": false },
                        { "type": "object", "properties": { "kind": { "const": "q" } }, "required": ["kind"], "additionalProperties": false }
                    ]
                }))
                .unwrap_or_else(|e| die(format!("schema: {e:?}")));
                misaka_palw_constraint::compile_v2::compile_v2(&schema).unwrap_or_else(|e| die(format!("compile: {e:?}"))).to_bytes()
            };
            job.version = PALW_FP_CONSTRAINT_VERSION;
            job.tokenizer_id = t.tokenizer_id;
            job.sampling_seed = PALW_DECODE_SEED_GREEDY;
            job.temperature_q = PALW_DECODE_TEMPERATURE_GREEDY;
            job.tail = Some(PalwFpJobTailV1::Constraint(PalwFpConstraintTailV1 { constraint: bytes, table_root: t.root() }));
            let path = table_out.clone().unwrap_or_else(|| outbox.join("floor.palwtokens"));
            std::fs::write(&path, palw_token_table_file_encode_v1(&t)).unwrap_or_else(|e| die(format!("{}: {e}", path.display())));
            eprintln!("token table: {} ({} ids, root {})", path.display(), vocab, hex_of(t.root()));
            table = Some(t);
        }
        _ => {
            job = job.into_v4(kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4::NOOP);
            job.version = version;
            let state = PalwFpPrefixStateV1 {
                state_root: palw_fp_prefix_state_root_v1(&floor, 3, &Hash64::from_u64_word(0xCAFE)),
                prefix_tokens: 3,
                class_id: floor,
            };
            job.tail = Some(PalwFpJobTailV1::Prefix(state));
        }
    }

    let prompt_usize: Vec<usize> = prompt.iter().map(|t| *t as usize).collect();
    let run = match &table {
        Some(t) => {
            let mask = PalwConstraintMaskV1::for_job(&job, t.clone()).unwrap_or_else(|e| die(format!("the table is not the job's: {e:?}")));
            palw_fp_with_constraint_scope_v1(Some(mask), || backend.execute_free_prompt(&job, &prompt_usize))
        }
        None => backend.execute_free_prompt(&job, &prompt_usize),
    }
    .unwrap_or_else(|e| die(format!("the floor refused the job: {e}")));

    let class = PalwFpClassFactsV3 {
        model_profile_id: Hash64::default(),
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: Hash64::default(),
        shape_profile_id: floor,
        cu_ruleset_id: Hash64::default(),
    };
    let commitment = palw_fp_commitment_v3(&job, &class, &run, &network_id, anchor_daa.saturating_add(200_000))
        .unwrap_or_else(|e| die(format!("the run does not commit: {e:?}")));
    let job_id = kaspa_consensus_core::palw_freeprompt_v3::fp_job_id_v3(&job);
    let stem = format!("fp-job-{}", &hex_of(job_id)[..16]);
    let result = PalwFpWorkerResultV3 {
        version: PALW_FP_V3_VERSION,
        request_hash: job_id,
        job: job.clone(),
        prompt_token_ids: prompt.clone(),
        trace_root: run.outcome.trace_root,
        output_root: run.outcome.output_root,
        schedule_root: commitment.schedule_root,
        execution_root: run.outcome.execution_root,
        trace_manifest_root: run.outcome.trace_manifest_root,
        trace_chunk_count: run.outcome.trace_chunk_count,
        trace_event_count: run.facts.decode_tokens_executed,
        decode_tokens_executed: run.facts.decode_tokens_executed,
        step_leaf_count: run.facts.step_leaf_count,
        stop_reason: run.facts.stop_reason,
        output_token_ids: run.output_token_ids.clone(),
        rendered: Vec::new(),
        model_load_ms: 0,
        execute_ms: 0,
    };
    let write = |name: &str, bytes: Vec<u8>| {
        let path = outbox.join(name);
        std::fs::write(&path, bytes).unwrap_or_else(|e| die(format!("{}: {e}", path.display())));
    };
    write(&format!("{stem}.result.borsh"), borsh::to_vec(&result).unwrap_or_else(|e| die(e)));
    write(&format!("{stem}.commitment-unsigned.borsh"), borsh::to_vec(&commitment).unwrap_or_else(|e| die(e)));
    write(&format!("{stem}.capture.bin"), run.outcome.material.clone());
    let _ = PalwFpCommitmentTxPayloadV3::claim_id; // the rail signs the payload; the claim id is read from the chain afterwards
    println!("{}", outbox.join(&stem).display());
}
