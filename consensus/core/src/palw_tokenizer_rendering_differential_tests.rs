//! RFC-0051 semantic parity with the existing sequential JSON court primitive.
//! These tests authenticate the same raw input. They do not equate the two
//! transports, prices, signed moves, or whole-claim admission rules.
use super::*;
use crate::{
    palw_fp_constraint_job_v1::PalwTokenTableV1,
    palw_rendering_stream_v1::LeafHashContinuationV1,
    palw_tokenizer_json_stream_v1::{
        PALW_TOKENIZER_RENDERING_DIGEST_DOMAIN_V1, PalwTokenizerJsonStreamV1, PalwTokenizerJsonVerdictV1,
        PalwTokenizerRenderingDigestV1, tests::Book,
    },
};

fn stream(raw: &[u8], identity: Hash64, token: u32, bound: u64, chunk: usize) -> Result<(PalwTokenizerJsonVerdictV1, u64), String> {
    let context = Hash64::from_u64_word(0x0051);
    let mut parser = PalwTokenizerJsonStreamV1::new(context, identity, raw.len() as u64, token);
    let mut book = Book::new();
    loop {
        if let Some((namespace, key)) = parser.pending_key() {
            if parser.key_events() == bound {
                parser.mark_work_overrun(context, bound)?;
            } else {
                let before = parser.key_insertions();
                parser.prove_key(context, &book.proof(namespace, key))?;
                if parser.key_insertions() != before {
                    book.insert(namespace, key);
                }
            }
        } else if parser.offset() < raw.len() as u64 {
            let offset = parser.offset() as usize;
            let consumed = parser.push(context, offset as u64, &raw[offset..(offset + chunk).min(raw.len())])?;
            assert!(consumed > 0 || parser.pending_key().is_some());
        } else {
            parser.seal()?;
            return parser.finish().map(|verdict| (verdict, parser.key_events()));
        }
        // Restart every step through the real bounded parser state codec.
        parser = borsh::from_slice(&borsh::to_vec(&parser).unwrap()).unwrap();
    }
}

fn statement(raw: &[u8], table: &PalwTokenTableV1) -> PalwTokenizerRenderingStatementV2 {
    PalwTokenizerRenderingStatementV2 {
        tokenizer_id: palw_tokenizer_raw_commitment_v1(raw),
        exact_raw_bytes: raw.len() as u64,
        table_root: table.root(),
        vocab: table.vocab(),
        rule: PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1,
    }
}

fn table(entries: Vec<Option<Vec<u8>>>) -> PalwTokenTableV1 {
    PalwTokenTableV1 { entries, eog_token_ids: vec![0, 2], tokenizer_id: Hash64::default() }
}

fn digest(bytes: &Option<Vec<u8>>) -> Option<PalwTokenizerRenderingDigestV1> {
    bytes.as_ref().map(|bytes| {
        let mut hash = LeafHashContinuationV1::new(PALW_TOKENIZER_RENDERING_DIGEST_DOMAIN_V1);
        hash.update(bytes);
        PalwTokenizerRenderingDigestV1 { length: bytes.len() as u64, digest: hash.finish() }
    })
}

#[test]
fn tokenizer_registration_differential_rendering_eog_and_key_work() {
    let cases = [
        (br#"{"added_tokens":[{"id":2,"content":"long"}],"model":{"vocab":{"A":0,"b":2},"merges":[]}}"#.as_slice(),
         table(vec![Some(b"A".to_vec()), Some(vec![]), Some(b"long".to_vec()), None])),
        (br#"{"model":{"vocab":{"\u0100":0,"\u6f22":2},"merges":[["a","b"],"a b"]},"added_tokens":[{"content":"\ud83d\ude00","id":2}]}"#,
         table(vec![Some(vec![0]), Some(vec![]), Some("😀".as_bytes().to_vec()), None])),
        (br#"{"model":{"vocab":{"A":0,"\u6f22":2},"merges":[]}}"#,
         table(vec![Some(b"A".to_vec()), Some(vec![]), None, None])),
    ];
    for (raw, table) in cases {
        let statement = statement(raw, &table);
        let eog = table.eog_opening().unwrap();
        let facts = derive_tokenizer_rendering_v2(statement, raw, &eog, ROOTED_TOKENIZER_LIMITS_V1).unwrap();
        for chunk in [1, 7, 65_536] {
            for (id, bytes) in table.entries.iter().enumerate() {
                let (verdict, events) = stream(raw, statement.tokenizer_id, id as u32, facts.key_events, chunk).unwrap();
                assert_eq!(verdict, PalwTokenizerJsonVerdictV1::Rendering(digest(bytes)));
                assert_eq!(events, facts.key_events);
            }
            let (verdict, _) = stream(raw, statement.tokenizer_id, 0, facts.key_events - 1, chunk).unwrap();
            assert_eq!(verdict, PalwTokenizerJsonVerdictV1::WorkBoundExceeded);
        }
        let low = PalwTokenizerRenderingBudgetV2 { key_events: facts.key_events - 1, ..ROOTED_TOKENIZER_LIMITS_V1 };
        assert_eq!(derive_tokenizer_rendering_v2(statement, raw, &eog, low).unwrap_err(), "tokenizer exceeds its key event limit");

        // EOG is separately authenticated by the table; raw JSON never selects it.
        let mut forged = eog.clone();
        forged.ids[1] = 1;
        assert!(!forged.verifies(&statement.table_root, statement.vocab));
        assert_eq!(
            derive_tokenizer_rendering_v2(statement, raw, &forged, ROOTED_TOKENIZER_LIMITS_V1).unwrap_err(),
            "tokenizer EOG does not open the registered table"
        );

        let mut wrong_table = table.clone();
        wrong_table.entries[2] = Some(b"wrong".to_vec());
        let wrong_statement = PalwTokenizerRenderingStatementV2 { table_root: wrong_table.root(), ..statement };
        assert!(
            derive_tokenizer_rendering_v2(wrong_statement, raw, &wrong_table.eog_opening().unwrap(), ROOTED_TOKENIZER_LIMITS_V1)
                .is_err()
        );
        let (actual, _) = stream(raw, statement.tokenizer_id, 2, facts.key_events, 7).unwrap();
        assert_ne!(actual, PalwTokenizerJsonVerdictV1::Rendering(digest(&wrong_table.entries[2])));
    }
}

#[test]
fn tokenizer_registration_differential_invalid_json_and_authenticated_eof() {
    let table = table(vec![Some(b"A".to_vec()), Some(vec![]), None, None]);
    let eog = table.eog_opening().unwrap();
    for raw in [
        br#"{"model":{"vocab":{"A":0,"\u0041":1},"merges":[]}}"#.as_slice(),
        br#"{"model":{"vocab":{"A":0,"B":0},"merges":[]}}"#,
        br#"{"model":{"vocab":{},"merges":[]},"added_tokens":[{"id":0,"content":"a"},{"id":0,"content":"b"}]}"#,
        br#"{"model":{"vocab":{"A":0},"merges":["missing_separator"]}}"#,
        br#"{"model":{"vocab":{"A":0"#,
        br#"{"model":{"vocab":{"A":0},"merges":[]}} trailing"#,
    ] {
        let statement = statement(raw, &table);
        assert!(derive_tokenizer_rendering_v2(statement, raw, &eog, ROOTED_TOKENIZER_LIMITS_V1).is_err());
        for chunk in [1, 7, 65_536] {
            assert_eq!(stream(raw, statement.tokenizer_id, 0, 1024, chunk).unwrap().0, PalwTokenizerJsonVerdictV1::Invalid);
            // A malformed but unauthenticated file cannot establish an offence.
            assert!(stream(raw, Hash64::default(), 0, 1024, chunk).is_err());
        }
    }
    let raw = br#"{"model":{"vocab":{"A":0},"merges":[]}}"#;
    let statement = statement(raw, &table);
    assert!(derive_tokenizer_rendering_v2(statement, &raw[..raw.len() - 1], &eog, ROOTED_TOKENIZER_LIMITS_V1).is_err());
    assert!(stream(&raw[..raw.len() - 1], statement.tokenizer_id, 0, 1024, 7).is_err());
}
