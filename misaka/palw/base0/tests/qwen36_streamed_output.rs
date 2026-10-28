//! **Genesis #8, streamed both ends (2026-10-16):** `qwen36-convert` can write its `PALWQ361` file into a pipe — a measuring pass
//! records the parameters and every tensor's code digest, the output pass builds the header with those parameters and writes
//! header and codes strictly in order (`Qwen36Writer::create_to`) — and `palwart pack --from-pipe` carries the stream into
//! `PALWART3` segments. The bytes must be the ones the one-pass run leaves in its file (`Qwen36Writer::create` + patch at the end):
//!
//! * the streamed writer, its header built with the measured parameters, writes exactly the patched file's bytes, and says
//!   exactly that many bytes beforehand (`file_bytes`, what `--from-bytes` is given);
//! * the output pass refuses a tensor whose codes differ from the measuring pass's (by BLAKE2b), before writing it;
//! * a sink cannot be patched (a streamed run that tried would leave placeholder parameters);
//! * packing the streamed bytes gives the root of packing the file.

#![allow(warnings)]
use misaka_palw_base0::qwen36::{Qwen36Writer, qwen36_dev_fixture};
use misaka_palw_tir_artifact::palwart_pack::{LegacyStreamV3, PalwArtPackInputsV3, pack_legacy_stream_v3, pack_legacy_v3};
use misaka_palw_tir_artifact::palwart_v3::family;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// A sink the test can read back.
#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Shared {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("misaka-q36-stream-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn the_streamed_writer_writes_the_patched_files_bytes_and_packs_to_its_root() {
    let a = qwen36_dev_fixture(4, 4);
    let plan: Vec<(String, usize)> = a.tensor_names().iter().map(|n| (n.to_string(), a.tensor(n).expect("present").len())).collect();
    let placeholders = a.params_map().clone();
    // "Measured" parameters: every row's bytes changed, widths kept (what the calibration patches in at the end).
    let measured: BTreeMap<String, Vec<u8>> =
        placeholders.iter().map(|(k, v)| (k.clone(), v.iter().enumerate().map(|(i, b)| b ^ (i as u8 | 1)).collect())).collect();

    // One pass into a file, patched at the end (the converter's local-file path).
    let dir = tmp("file");
    let file = dir.join("one-pass.palwq36");
    let mut w = Qwen36Writer::create(&file, &a.shape, &a.rope, &placeholders, plan.clone()).expect("created");
    for (name, _) in &plan {
        w.push(name, &a.tensor(name).unwrap()).expect("pushed");
    }
    w.finish_with_params(&measured).expect("patched");
    let one_pass = std::fs::read(&file).unwrap();

    // The measuring pass: a sink, recording each tensor's digest.
    let mut probe = Qwen36Writer::create_to(Box::new(std::io::sink()), &a.shape, &a.rope, &placeholders, plan.clone()).expect("sink");
    for (name, _) in &plan {
        probe.push(name, &a.tensor(name).unwrap()).expect("pushed");
    }
    let digests = probe.digests().clone();
    assert_eq!(digests.len(), plan.len());

    // The output pass: the header built with the measured parameters, every tensor checked against the measuring pass.
    let mut header_params = placeholders.clone();
    for (k, v) in &measured {
        header_params.insert(k.clone(), v.clone());
    }
    let sink = Shared::default();
    let mut out = Qwen36Writer::create_to(Box::new(sink.clone()), &a.shape, &a.rope, &header_params, plan.clone())
        .expect("sink")
        .with_expected_digests(digests.clone());
    assert_eq!(out.file_bytes(), one_pass.len(), "the length the packer is told before the first byte");
    for (name, _) in &plan {
        out.push(name, &a.tensor(name).unwrap()).expect("pushed");
    }
    out.finish().expect("closed");
    let streamed = sink.0.lock().unwrap().clone();
    assert!(streamed == one_pass, "the streamed bytes are the patched file's bytes");

    // A tensor whose codes differ from the measuring pass's is refused before it is written.
    let mut bad = Qwen36Writer::create_to(Box::new(std::io::sink()), &a.shape, &a.rope, &header_params, plan.clone())
        .expect("sink")
        .with_expected_digests(digests);
    let mut first = a.tensor(&plan[0].0).unwrap().to_vec();
    first[0] = first[0].wrapping_add(1);
    assert!(bad.push(&plan[0].0, &first).unwrap_err().to_string().contains("not deterministic"));

    // A sink cannot be patched.
    let mut unpatched = Qwen36Writer::create_to(Box::new(std::io::sink()), &a.shape, &a.rope, &placeholders, plan.clone()).unwrap();
    for (name, _) in &plan {
        unpatched.push(name, &a.tensor(name).unwrap()).unwrap();
    }
    assert!(unpatched.finish_with_params(&measured).is_err());

    // The container: packing the streamed bytes (no pin: a new artifact) gives the root of packing the file.
    let inputs = |out: &std::path::Path| PalwArtPackInputsV3 {
        source_dir: None,
        manifest: serde_json::json!({"schema": "misaka.genesis8.palwart-manifest.v1", "name": "Q36-PALW"}),
        family: family::NONE,
        out_dir: out.to_path_buf(),
        stem: "Q36-PALW".into(),
        segment_bytes: 1 << 16,
        license_files: Vec::new(),
        calibration_files: Vec::new(),
        provenance: serde_json::json!({}),
    };
    let from_file = pack_legacy_v3(&file, &inputs(&dir.join("a")), Box::new(|_| Ok(()))).expect("packed from the file");
    let mut reader = std::io::Cursor::new(streamed);
    let from_pipe = pack_legacy_stream_v3(
        LegacyStreamV3 { reader: &mut reader, len: one_pass.len() as u64, name: "qwen36.palwq36".into(), sha256: None },
        &inputs(&dir.join("b")),
        Box::new(|_| Ok(())),
    )
    .expect("packed from the stream");
    assert_eq!(from_pipe.written.root, from_file.written.root, "one artifact identity, two transports");
    assert!(from_file.written.header.segments.len() > 1, "the stream is cut into several segments");
    let _ = std::fs::remove_dir_all(&dir);
}
