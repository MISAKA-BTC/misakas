//! `PALWART3` (genesis 8): the container's own rules, without any converter.
//!
//! * a written artifact opens, every entry reads back digest-checked across segment cuts;
//! * the root is a function of content, not of the segment size (storage) or the provenance;
//! * a changed byte anywhere is refused: a segment's SHA-256, an entry digest, the index root;
//! * the segment hook sees every segment once, in order, and may delete it (streamed upload);
//! * legacy magics are named, never opened as this container.
//!
//! Written, not run here (the validation lane compiles and runs it).

use misaka_palw_tir_artifact::palwart_v3::*;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "palwart-v3-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Deterministic bytes: an LCG, so the test needs no RNG crate.
fn bytes(n: usize, seed: u64) -> Vec<u8> {
    let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (0..n)
        .map(|_| {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (x >> 33) as u8
        })
        .collect()
}

fn manifest() -> Vec<u8> {
    canonical_json_v3(&serde_json::json!({
        "source": {"repo": "example/model", "revision": "0123456789abcdef0123456789abcdef01234567"},
        "license": {"id": "apache-2.0"},
        "name": "Example-PALW",
    }))
    .into_bytes()
}

/// Write a three-section artifact (weights ~2.6 MiB in two entries, a tokenizer, a license) with
/// `segment` bytes per segment; the hook records what it saw and optionally deletes.
fn write(dir: &std::path::Path, segment: u64, delete: bool, provenance: &str) -> (PalwArtWrittenV3, Vec<PalwArtSegmentDoneV3>) {
    let mut seen = Vec::new();
    let hook: PalwArtSegmentHookV3<'_> = Box::new(|d: &PalwArtSegmentDoneV3| -> Result<(), String> {
        assert!(d.path.is_file(), "the hook sees a complete file");
        assert_eq!(std::fs::metadata(&d.path).unwrap().len(), d.bytes);
        seen.push(d.clone());
        if delete {
            std::fs::remove_file(&d.path).map_err(|e| e.to_string())?;
        }
        Ok(())
    });
    let mut w = PalwArtWriterV3::create(dir, "example", segment, hook).unwrap();
    w.begin_section(section_kind::TIR_INVENTORY, family::QWEN35_GDN_HYBRID, "tir-inventory").unwrap();
    // An i8 [1600, 1024] instance written in uneven pieces, then an i32 [3, 7] one.
    let a = bytes(1600 * 1024, 1);
    w.begin_entry("w.q", Some(0), dtype::I8, vec![1600, 1024], encoding::TIR_INSTANCE_LE, a.len() as u64).unwrap();
    for piece in a.chunks(99_991) {
        w.write(piece).unwrap();
    }
    w.end_entry().unwrap();
    let b = bytes(3 * 7 * 4, 2);
    w.put_entry("w.s", Some(0), dtype::I32, vec![3, 7], encoding::TIR_INSTANCE_LE, &b).unwrap();
    let c = bytes(1 << 20, 3);
    w.put_entry("emb", None, dtype::I8, vec![1024, 1024], encoding::TIR_INSTANCE_LE, &c).unwrap();
    w.end_section(inner_scheme::TIR_INVENTORY_ROOT_V1, [7u8; 64], 123).unwrap();
    w.begin_section(section_kind::TOKENIZER, family::NONE, "tokenizer").unwrap();
    w.put_entry("tokenizer.json", None, dtype::BYTES, vec![17], encoding::JSON, b"{\"model\":\"bpe\"} ").unwrap();
    w.end_section(inner_scheme::NONE, [0; 64], 0).unwrap();
    w.begin_section(section_kind::LICENSE, family::NONE, "license").unwrap();
    w.put_entry("added/LICENSE", None, dtype::BYTES, vec![10], encoding::UTF8_TEXT, b"Apache-2.0").unwrap();
    w.end_section(inner_scheme::NONE, [0; 64], 0).unwrap();
    let program = PalwArtProgramV3 {
        kind: program_kind::TIR_V1,
        bytes: vec![1, 2, 3],
        kernel_id: [9; 64],
        kernel_version: 0,
        tokenizer_id: [5; 64],
    };
    let written = w.finish(manifest(), program, Vec::new(), String::new(), provenance.as_bytes().to_vec()).unwrap();
    (written, seen)
}

#[test]
fn a_written_artifact_opens_and_reads_back_across_segment_cuts() {
    let dir = tmp("roundtrip");
    let (written, seen) = write(&dir, 1 << 20, false, "{}");
    let art = PalwArtV3::open(&written.index_path).unwrap();
    assert_eq!(art.root, written.root);
    assert_eq!(art.root, palwart_root_v3(&art.header));
    assert!(art.header.segments.len() >= 3, "~2.6 MiB of data in 1 MiB segments");
    assert_eq!(seen.len(), art.header.segments.len(), "the hook saw every segment once");
    for (k, d) in seen.iter().enumerate() {
        assert_eq!(d.index as usize, k, "in order");
        assert_eq!(d.sha256, art.header.segments[k].sha256);
    }
    assert!(art.verify_segments(true).unwrap().is_empty());
    art.verify_entries().unwrap();
    let (si, ei) = art.find_entry(section_kind::TIR_INVENTORY, "w.q").unwrap();
    assert_eq!(art.read_entry(si, ei).unwrap(), bytes(1600 * 1024, 1));
    let (si, ei) = art.find_entry(section_kind::TIR_INVENTORY, "emb").unwrap();
    assert_eq!(art.read_entry(si, ei).unwrap(), bytes(1 << 20, 3));
    for s in &art.header.sections {
        for e in &s.entries {
            assert_eq!(e.offset % PALWART_ALIGN_V3, 0);
        }
    }
    assert_eq!(sniff_format_v3(&written.index_path).unwrap(), PalwArtifactFormatV3::PalwArtV3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_root_is_content_not_storage_or_provenance() {
    let (d1, d2) = (tmp("root-a"), tmp("root-b"));
    let (a, _) = write(&d1, 1 << 20, false, "{\"host\":\"a\"}");
    let (b, _) = write(&d2, 3 << 20, false, "{\"host\":\"b\",\"time\":1}");
    assert_ne!(a.header.segments.len(), b.header.segments.len(), "different cuts");
    assert_eq!(a.root, b.root, "the same content has the same root whatever the segment size and provenance");
    let mut h = a.header.clone();
    h.manifest = canonical_json_v3(&serde_json::json!({"license": {"id": "mit"}})).into_bytes();
    assert_ne!(palwart_root_v3(&h), a.root, "the manifest is identity");
    let mut h = a.header.clone();
    h.sections[0].inner_root[0] ^= 1;
    assert_ne!(palwart_root_v3(&h), a.root, "a section's inner root is identity");
    let mut h = a.header.clone();
    h.sections[0].entries[1].digest[0] ^= 1;
    assert_ne!(palwart_root_v3(&h), a.root, "an entry digest is identity");
    let _ = std::fs::remove_dir_all(&d1);
    let _ = std::fs::remove_dir_all(&d2);
}

#[test]
fn a_changed_byte_is_refused_where_it_lies() {
    let dir = tmp("tamper");
    let (written, _) = write(&dir, 1 << 20, false, "{}");
    // A segment byte: its SHA-256 and the entry digest both catch it.
    let seg = dir.join(segment_file_name_v3("example", 1));
    let mut s = std::fs::read(&seg).unwrap();
    s[12345] ^= 0x40;
    std::fs::write(&seg, &s).unwrap();
    let art = PalwArtV3::open(&written.index_path).unwrap();
    assert!(matches!(art.verify_segments(true), Err(PalwArtErrorV3::Segment { index: 1, .. })));
    assert!(art.verify_entries().is_err());
    // A manifest byte inside the index: the root no longer reproduces. (Segment hashes in the index are
    // storage, not identity: a forged one fails `verify_segments` against the bytes, and forged bytes
    // with a matching forged hash still fail the entry digests, which ARE in the root.)
    let mut idx = std::fs::read(&written.index_path).unwrap();
    let needle = b"example/model";
    let at = idx.windows(needle.len()).position(|w| w == needle).expect("the manifest is in the index");
    idx[at] ^= 1;
    assert!(matches!(parse_index_v3(&idx), Err(PalwArtErrorV3::Root { .. })));
    // The trailing root itself.
    let mut idx = std::fs::read(&written.index_path).unwrap();
    let n = idx.len();
    idx[n - 1] ^= 1;
    assert!(matches!(parse_index_v3(&idx), Err(PalwArtErrorV3::Root { .. })));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_streamed_upload_deletes_segments_and_the_index_still_names_them() {
    let dir = tmp("stream");
    let (written, seen) = write(&dir, 1 << 20, true, "{}");
    assert!(seen.iter().all(|d| !d.path.exists()), "every segment was handed off and deleted");
    let art = PalwArtV3::open(&written.index_path).unwrap();
    let missing = art.verify_segments(false).unwrap();
    assert_eq!(missing.len(), art.header.segments.len());
    assert!(art.verify_segments(true).is_err(), "a full verification needs every segment");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_writer_refuses_what_the_format_cannot_hold() {
    let dir = tmp("refuse");
    let hook: PalwArtSegmentHookV3<'_> = Box::new(|_: &PalwArtSegmentDoneV3| -> Result<(), String> { Ok(()) });
    assert!(PalwArtWriterV3::create(&dir, "x", 1024, hook).is_err(), "a segment below the minimum");
    let hook: PalwArtSegmentHookV3<'_> = Box::new(|_: &PalwArtSegmentDoneV3| -> Result<(), String> { Ok(()) });
    let mut w = PalwArtWriterV3::create(&dir, "x", 1 << 20, hook).unwrap();
    assert!(w.begin_entry("e", None, dtype::BYTES, vec![3], encoding::RAW_LE, 3).is_err(), "an entry outside a section");
    w.begin_section(section_kind::SOURCE_METADATA, family::NONE, "m").unwrap();
    assert!(w.begin_entry("e", None, dtype::I32, vec![3], encoding::RAW_LE, 11).is_err(), "bytes ≠ shape × width");
    w.begin_entry("e", None, dtype::I32, vec![3], encoding::RAW_LE, 12).unwrap();
    w.write(&[0u8; 8]).unwrap();
    assert!(w.end_entry().is_err(), "an entry short of its declared bytes");
    let hook: PalwArtSegmentHookV3<'_> = Box::new(|_: &PalwArtSegmentDoneV3| -> Result<(), String> { Err("upload failed".into()) });
    let mut w = PalwArtWriterV3::create(&tmp("refuse-hook"), "y", 1 << 20, hook).unwrap();
    w.begin_section(section_kind::SOURCE_METADATA, family::NONE, "m").unwrap();
    let big = vec![0u8; (1 << 20) + 1];
    w.begin_entry("big", None, dtype::BYTES, vec![big.len() as u64], encoding::RAW_LE, big.len() as u64).unwrap();
    assert!(matches!(w.write(&big), Err(PalwArtErrorV3::Hook(_))), "a failing hook stops the conversion");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn legacy_magics_are_named_and_never_opened_as_v3() {
    for (m, f) in [
        (&b"PALWB0A1"[..], PalwArtifactFormatV3::Base0V1),
        (b"PALWB0A2", PalwArtifactFormatV3::Base0A16V2),
        (b"PALWTIR1", PalwArtifactFormatV3::TirV1),
        (b"PALWTIR2", PalwArtifactFormatV3::TirV2),
        (b"PALWTIRS", PalwArtifactFormatV3::TirSection),
        (b"PALWQ361", PalwArtifactFormatV3::PalwQ361),
        (b"GGUF\x03\0\0\0", PalwArtifactFormatV3::Unknown),
    ] {
        assert_eq!(PalwArtifactFormatV3::of_magic(m), f);
        assert_eq!(f.is_legacy(), !matches!(f, PalwArtifactFormatV3::Unknown));
        let mut fake = m.to_vec();
        fake.extend_from_slice(&[0u8; 80]);
        assert!(matches!(parse_index_v3(&fake), Err(PalwArtErrorV3::Magic)));
    }
}

#[test]
fn canonical_json_sorts_keys_at_every_depth() {
    let v = serde_json::json!({"b": 1, "a": {"d": [3, {"z": 0, "y": 1}], "c": "x"}});
    assert_eq!(canonical_json_v3(&v), r#"{"a":{"c":"x","d":[3,{"y":1,"z":0}]},"b":1}"#);
}
