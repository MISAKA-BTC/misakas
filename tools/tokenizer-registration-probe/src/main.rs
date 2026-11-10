//! Offline parser measurements for RFC-0051. No RPC, keys or chain writes.
//! Fixture construction is a separate invocation so it does not inflate the
//! measured process's peak RSS. This is not a full native-fold qualification.
use kaspa_consensus_core::{
    palw_fp_constraint_job_v1::{PalwTokenTableEogOpeningV1, PalwTokenTableV1, palw_token_table_file_decode_v1},
    palw_tokenizer_rendering_v1::{PalwTokenizerRenderingRuleV1, palw_tokenizer_raw_commitment_v1},
    palw_tokenizer_rendering_v2::{PalwTokenizerRenderingBudgetV2, PalwTokenizerRenderingStatementV2, derive_tokenizer_rendering_v2},
};
use serde_json::json;
use std::{fs, path::Path, time::Instant};

const RAW_MAX: usize = 8 * 1024 * 1024;
const VOCAB_MAX: usize = 262_144;
const BUDGET: PalwTokenizerRenderingBudgetV2 = PalwTokenizerRenderingBudgetV2 {
    raw_bytes: RAW_MAX as u64,
    vocab: VOCAB_MAX as u32,
    key_events: 4 * 1024 * 1024,
    eog_ids: 4096,
    total_rendering_bytes: RAW_MAX as u64,
};

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(crate = "borsh")]
struct Case {
    statement: PalwTokenizerRenderingStatementV2,
    eog: PalwTokenTableEogOpeningV1,
    expected_error: Option<String>,
}

fn write_case(dir: &Path, name: &str, raw: &[u8], table: &PalwTokenTableV1, expected_error: Option<&str>) {
    assert!(raw.len() <= RAW_MAX);
    let case = Case {
        statement: PalwTokenizerRenderingStatementV2 {
            tokenizer_id: palw_tokenizer_raw_commitment_v1(raw),
            exact_raw_bytes: raw.len() as u64,
            table_root: table.root(),
            vocab: table.vocab(),
            rule: PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1,
        },
        eog: table.eog_opening().expect("fixture EOG opening"),
        expected_error: expected_error.map(str::to_owned),
    };
    fs::write(dir.join(format!("{name}.raw")), raw).unwrap();
    fs::write(dir.join(format!("{name}.case")), borsh::to_vec(&case).unwrap()).unwrap();
    println!(
        "{}",
        json!({"case":name,"rawBytes":raw.len(),"vocab":table.vocab(),"eogIds":case.eog.ids.len(),"expectedError":case.expected_error})
    );
}

fn table(entries: Vec<Option<Vec<u8>>>, ids: Vec<u32>) -> PalwTokenTableV1 {
    PalwTokenTableV1 { entries, eog_token_ids: ids, tokenizer_id: Default::default() }
}

fn pad(mut raw: Vec<u8>) -> Vec<u8> {
    assert!(raw.len() <= RAW_MAX);
    raw.resize(RAW_MAX, b' ');
    raw
}

fn prepare(dir: &Path, actual_raw: &Path, actual_table: &Path) {
    fs::create_dir(dir).expect("output must be a new directory");
    let raw = fs::read(actual_raw).unwrap();
    let actual = palw_token_table_file_decode_v1(&fs::read(actual_table).unwrap()).unwrap();
    assert_eq!(actual.tokenizer_id, palw_tokenizer_raw_commitment_v1(&raw));
    write_case(dir, "actual", &raw, &actual, None);

    let mut vocab = serde_json::Map::new();
    let mut entries = Vec::with_capacity(VOCAB_MAX);
    for id in 0..VOCAB_MAX {
        let token = format!("t{id:06}");
        vocab.insert(token.clone(), json!(id));
        entries.push(Some(token.into_bytes()));
    }
    let dense = pad(serde_json::to_vec(&json!({"model":{"vocab":vocab,"merges":[]},"added_tokens":[]})).unwrap());
    let mut dense_table = table(entries, (0..4096).collect());
    write_case(dir, "maximum_dense", &dense, &dense_table, None);
    dense_table.entries[VOCAB_MAX - 1] = Some(b"mismatch".to_vec());
    write_case(
        dir,
        "last_leaf_mismatch",
        &dense,
        &dense_table,
        Some("the prefix is not the committed one: the accused's path disagrees with it at level 18"),
    );
    drop(dense_table);

    let prefix = b"{\"model\":{\"vocab\":{\"";
    let suffix = b"\":0},\"merges\":[]},\"added_tokens\":[]}";
    let token_len = RAW_MAX - prefix.len() - suffix.len();
    let mut giant = prefix.to_vec();
    giant.resize(prefix.len() + token_len, b'A');
    giant.extend_from_slice(suffix);
    write_case(dir, "giant_string", &giant, &table(vec![Some(vec![b'A'; token_len])], vec![0]), None);

    let sparse = br#"{"model":{"vocab":{"A":262143},"merges":[]},"added_tokens":[]}"#;
    let mut sparse_entries = vec![Some(Vec::new()); VOCAB_MAX];
    sparse_entries[VOCAB_MAX - 1] = Some(b"A".to_vec());
    let sparse_table = table(sparse_entries, vec![VOCAB_MAX as u32 - 1]);
    write_case(dir, "sparse_legal", sparse, &sparse_table, None);
    write_case(
        dir,
        "sparse_out_of_range",
        br#"{"model":{"vocab":{"A":4294967295},"merges":[]}}"#,
        &sparse_table,
        Some("tokenizer names a token outside the registered vocabulary"),
    );

    let small_table = table(vec![Some(b"A".to_vec())], vec![0]);
    let mut repeated = br#"{"model":{"vocab":{"A":0},"merges":[]},"padding":""#.to_vec();
    let tail = br#"","model":{}}"#;
    repeated.resize(RAW_MAX - tail.len(), b'x');
    repeated.extend_from_slice(tail);
    write_case(dir, "duplicate_key_last", &repeated, &small_table, Some("malformed JSON or duplicate object key"));

    let mut many = br#"{"model":{"vocab":{"A":0},"merges":[]}"#.to_vec();
    for i in 0..1_000_000 {
        let entry = format!(",\"k{i}\":null");
        if many.len() + entry.len() + 1 > RAW_MAX {
            break;
        }
        many.extend_from_slice(entry.as_bytes());
    }
    many.push(b'}');
    let many = pad(many);
    write_case(dir, "many_keys", &many, &small_table, None);
}

fn measure(dir: &Path, name: &str, repetitions: u32) {
    assert!((1..=100).contains(&repetitions));
    // Local, self-generated bounded inputs. No transport decoder or authority.
    let raw = fs::read(dir.join(format!("{name}.raw"))).unwrap();
    assert!(raw.len() <= RAW_MAX);
    let bytes = fs::read(dir.join(format!("{name}.case"))).unwrap();
    assert!(bytes.len() <= 128 * 1024);
    let case: Case = borsh::from_slice(&bytes).unwrap();
    let scratch = raw.len() as u64 * 128 + 4 * case.eog.ids.len() as u64 + 16_384;
    for iteration in 0..repetitions {
        let start = Instant::now();
        let result = derive_tokenizer_rendering_v2(case.statement, &raw, &case.eog, BUDGET);
        let elapsed = start.elapsed();
        assert_eq!(result.as_ref().err(), case.expected_error.as_ref(), "fixture outcome changed: {name}");
        println!(
            "{}",
            json!({
                "case":name,"iteration":iteration,"elapsedNs":elapsed.as_nanos(),
                "rawBytes":raw.len(),"vocab":case.statement.vocab,"eogIds":case.eog.ids.len(),
                "conservativeScratchBytes":scratch,"ok":result.is_ok(),"error":result.as_ref().err(),
                "keyEvents":result.as_ref().ok().map(|v| v.key_events),
                "totalRenderingBytes":result.as_ref().ok().map(|v| v.total_rendering_bytes),
                "maximumRenderingBytes":result.as_ref().ok().map(|v| v.maximum_rendering_bytes)
            })
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("prepare") if args.len() == 5 => prepare(Path::new(&args[2]), Path::new(&args[3]), Path::new(&args[4])),
        Some("run") if args.len() == 5 => measure(Path::new(&args[2]), &args[3], args[4].parse().unwrap()),
        _ => panic!("usage: prepare NEW_DIR RAW_TOKENIZER TOKEN_TABLE | run DIR CASE REPETITIONS"),
    }
}
