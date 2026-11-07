use misaka_palw_tir_artifact::metadata_v16::*;
#[test]
fn correctly_rounded_profile_is_explicit_and_preserves_ieee_bits() {
    let text = r#"{"logits_scale":5.960464477539063e-8,"nested":[0.1,1.2345678901234567,-0.0],"count":18446744073709551615}"#;
    let exact = parse_metadata_v16(text, MetadataParseProfileV16::CorrectlyRoundedV16).unwrap();
    assert_eq!(exact["logits_scale"].as_f64().unwrap().to_bits(), (2f64.powi(-24)).to_bits());
    assert_eq!(exact["nested"][2].as_f64().unwrap().to_bits(), (-0.0f64).to_bits());
    assert_eq!(exact["count"].as_u64().unwrap(), u64::MAX);
    let legacy = parse_metadata_v16(text, MetadataParseProfileV16::LegacyBuildV1).unwrap();
    assert_eq!(legacy, serde_json::from_str::<serde_json::Value>(text).unwrap());
    let mut marked = exact.clone();
    record_metadata_profile_v16(&mut marked, MetadataParseProfileV16::CorrectlyRoundedV16).unwrap();
    let reread = parse_recorded_metadata_v16(&marked.to_string()).unwrap();
    assert_eq!(reread["logits_scale"].as_f64().unwrap().to_bits(), exact["logits_scale"].as_f64().unwrap().to_bits());
    assert!(record_metadata_profile_v16(&mut marked, MetadataParseProfileV16::LegacyBuildV1).is_err());
}
#[test]
fn legacy_metadata_image_is_unchanged_and_unknown_profiles_are_refused() {
    let text = r#"{"logits_scale":5.960464477539063e-8,"name":"old"}"#;
    assert_eq!(parse_recorded_metadata_v16(text).unwrap(), serde_json::from_str::<serde_json::Value>(text).unwrap());
    let mut old = parse_recorded_metadata_v16(text).unwrap();
    let before = old.to_string();
    record_metadata_profile_v16(&mut old, MetadataParseProfileV16::LegacyBuildV1).unwrap();
    assert_eq!(old.to_string(), before);
    for marker in [
        r#"{"schema":"wrong","profile":"correctly-rounded-v16"}"#,
        r#"{"schema":"misaka.palw.artifact-metadata-parsing.v16","profile":"guess"}"#,
        r#"{"schema":"misaka.palw.artifact-metadata-parsing.v16","profile":"correctly-rounded-v16","extra":true}"#,
    ] {
        assert!(parse_recorded_metadata_v16(&format!("{{\"metadata_parsing\":{marker}}}")).is_err());
    }
}
#[test]
fn oversized_nested_nonfinite_or_inexact_integer_metadata_is_refused() {
    for text in ["1e400", "18446744073709551616", "-9223372036854775809"] {
        assert!(parse_metadata_v16(text, MetadataParseProfileV16::CorrectlyRoundedV16).is_err());
    }
    let huge = format!("\"{}\"", "x".repeat(MAX_METADATA_BYTES_V16));
    assert!(parse_metadata_v16(&huge, MetadataParseProfileV16::CorrectlyRoundedV16).is_err());
    let nested = format!("{}0{}", "[".repeat(130), "]".repeat(130));
    assert!(parse_metadata_v16(&nested, MetadataParseProfileV16::CorrectlyRoundedV16).is_err());
}
