//! `palwart` — the one `PALWART3` artifact container of the genesis models (2026-10-15).
//!
//! ```text
//! palwart pack (--from <legacy file> | --from-url <pinned resolve URL> --from-bytes N --from-sha256 HEX [--from-name P]
//!               | --from-pipe <FIFO or -> --from-bytes N [--from-sha256 HEX] [--from-name P])
//!              --out-dir DIR --stem STEM --manifest manifest.json
//!              [--source-dir CHECKPOINT] [--family qwen2-dense|qwen35-gdn-hybrid|…] [--segment-mib 2048]
//!              [--license-file P]… [--calibration-file P]… [--provenance P]
//!              [--on-segment PROGRAM [--on-segment-arg A]… [--delete-after-hook]] [--json]
//! palwart inspect <index.palwart> [--data-dir DIR]
//! palwart verify <index.palwart> [--data-dir DIR]
//! palwart root <index.palwart>
//! palwart sniff <file>
//! palwart extract-legacy <index.palwart> --out FILE [--data-dir DIR]
//! ```
//!
//! `pack` carries an artifact a legacy converter wrote (`PALWB0A1/A2` from `qwen25-convert --a16`,
//! `PALWTIR1` from `palw-tir-convert`, `PALWQ361`) into the container without touching a weight
//! byte, adds the tokenizer, chat template, source metadata, license and calibration files read as
//! data from `--source-dir`, and writes the caller's manifest canonically. With `--on-segment`, every
//! finished segment is handed to PROGRAM (environment `PALWART_SEGMENT_PATH`, `PALWART_SEGMENT_INDEX`,
//! `PALWART_SEGMENT_BYTES`, `PALWART_SEGMENT_SHA256`, `PALWART_STEM`, `PALWART_OUT_DIR`) before the
//! next one is written — the genesis pipeline uploads it there — and `--delete-after-hook` removes it
//! once the program exits 0. The root printed is the artifact's identity (`palwart_root_v3`).
//!
//! This binary lives in the light `misaka-palw-tir-artifact` crate (no consensus or node crate), so it
//! builds on its own: `cargo build --release -p misaka-palw-tir-artifact --bin palwart`.

#![allow(warnings)]
use misaka_palw_tir_artifact::palwart_pack::{
    LegacyStreamV3, PalwArtPackInputsV3, extract_legacy_v3, pack_legacy_stream_v3, pack_legacy_v3, summary_json_v3, verify_v3,
};
use misaka_palw_tir_artifact::palwart_v3::{self as v3, PalwArtSegmentDoneV3, PalwArtV3};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const USAGE: &str = "usage: palwart pack|inspect|verify|root|sniff|extract-legacy … (see the source header)";

struct Args {
    positional: Vec<String>,
    flags: BTreeMap<String, Vec<String>>,
    switches: Vec<String>,
}

/// Flags that take no value.
const SWITCHES: &[&str] = &["--json", "--delete-after-hook"];

fn parse(argv: Vec<String>) -> Result<Args, String> {
    let mut a = Args { positional: Vec::new(), flags: BTreeMap::new(), switches: Vec::new() };
    let mut it = argv.into_iter();
    while let Some(k) = it.next() {
        if SWITCHES.contains(&k.as_str()) {
            a.switches.push(k);
        } else if k.starts_with("--") {
            let v = it.next().ok_or_else(|| format!("{k} wants a value"))?;
            a.flags.entry(k).or_default().push(v);
        } else {
            a.positional.push(k);
        }
    }
    Ok(a)
}

impl Args {
    fn one(&self, k: &str) -> Result<Option<String>, String> {
        match self.flags.get(k).map(Vec::as_slice) {
            None | Some([]) => Ok(None),
            Some([v]) => Ok(Some(v.clone())),
            Some(_) => Err(format!("{k} given more than once")),
        }
    }
    fn need(&self, k: &str) -> Result<String, String> {
        self.one(k)?.ok_or_else(|| format!("{k} is required"))
    }
    fn many(&self, k: &str) -> Vec<String> {
        self.flags.get(k).cloned().unwrap_or_default()
    }
    fn on(&self, k: &str) -> bool {
        self.switches.iter().any(|s| s == k)
    }
    fn check_known(&self, known: &[&str]) -> Result<(), String> {
        for k in self.flags.keys() {
            if !known.contains(&k.as_str()) {
                return Err(format!("unknown option {k}"));
            }
        }
        Ok(())
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn read_json(p: &Path) -> Result<serde_json::Value, String> {
    let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
    if bytes.len() > 16 << 20 {
        return Err(format!("{}: larger than 16 MiB", p.display()));
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", p.display()))
}

fn open(a: &Args) -> Result<PalwArtV3, String> {
    let index = PathBuf::from(a.positional.get(1).ok_or("the index path is required")?);
    match a.one("--data-dir")? {
        Some(d) => PalwArtV3::open_with_data(&index, Path::new(&d)),
        None => PalwArtV3::open(&index),
    }
    .map_err(|e| format!("{}: {e}", index.display()))
}

fn pack(a: &Args) -> Result<serde_json::Value, String> {
    a.check_known(&[
        "--from",
        "--from-url",
        "--from-bytes",
        "--from-sha256",
        "--from-name",
        "--from-pipe",
        "--out-dir",
        "--stem",
        "--manifest",
        "--source-dir",
        "--family",
        "--segment-mib",
        "--license-file",
        "--calibration-file",
        "--provenance",
        "--on-segment",
        "--on-segment-arg",
    ])?;
    let legacy = a.one("--from")?.map(PathBuf::from);
    let from_url = a.one("--from-url")?;
    let from_pipe = a.one("--from-pipe")?;
    if [legacy.is_some(), from_url.is_some(), from_pipe.is_some()].iter().filter(|x| **x).count() != 1 {
        return Err("give exactly one of --from <file>, --from-url <pinned resolve URL> and --from-pipe <FIFO or ->".into());
    }
    let out_dir = PathBuf::from(a.need("--out-dir")?);
    let stem = a.need("--stem")?;
    let manifest = read_json(Path::new(&a.need("--manifest")?))?;
    let family = match a.one("--family")? {
        Some(f) => v3::family::from_name(&f).ok_or_else(|| format!("--family {f}: unknown"))?,
        None => v3::family::NONE,
    };
    let segment_bytes = match a.one("--segment-mib")? {
        Some(m) => m.parse::<u64>().map_err(|e| format!("--segment-mib: {e}"))? << 20,
        None => v3::PALWART_DEFAULT_SEGMENT_BYTES_V3,
    };
    let provenance = match a.one("--provenance")? {
        Some(p) => read_json(Path::new(&p))?,
        None => serde_json::json!({}),
    };
    let inputs = PalwArtPackInputsV3 {
        source_dir: a.one("--source-dir")?.map(PathBuf::from),
        manifest,
        family,
        out_dir: out_dir.clone(),
        stem: stem.clone(),
        segment_bytes,
        license_files: a.many("--license-file").into_iter().map(PathBuf::from).collect(),
        calibration_files: a.many("--calibration-file").into_iter().map(PathBuf::from).collect(),
        provenance,
    };
    let program = a.one("--on-segment")?;
    let program_args = a.many("--on-segment-arg");
    let delete = a.on("--delete-after-hook");
    if delete && program.is_none() {
        return Err("--delete-after-hook without --on-segment would lose the segments".into());
    }
    let hook_out = out_dir.clone();
    let hook_stem = stem.clone();
    let hook = Box::new(|d: &PalwArtSegmentDoneV3| -> Result<(), String> {
        eprintln!("palwart: segment {} ({} bytes, sha256 {}) {}", d.index, d.bytes, hex(&d.sha256), d.path.display());
        if let Some(prog) = &program {
            let status = std::process::Command::new(prog)
                .args(&program_args)
                .env("PALWART_SEGMENT_PATH", &d.path)
                .env("PALWART_SEGMENT_INDEX", d.index.to_string())
                .env("PALWART_SEGMENT_BYTES", d.bytes.to_string())
                .env("PALWART_SEGMENT_SHA256", hex(&d.sha256))
                .env("PALWART_STEM", &hook_stem)
                .env("PALWART_OUT_DIR", &hook_out)
                .status()
                .map_err(|e| format!("{prog}: {e}"))?;
            if !status.success() {
                return Err(format!("{prog} exited {status} on segment {}", d.index));
            }
            if delete {
                std::fs::remove_file(&d.path).map_err(|e| format!("{}: {e}", d.path.display()))?;
            }
        }
        Ok(())
    });
    let packed = match (&legacy, &from_url) {
        (Some(legacy), _) => pack_legacy_v3(legacy, &inputs, hook)?,
        (None, None) => {
            // A converter writing into a pipe now (`qwen36-convert --measured-in … --out FIFO`): read once, in order, never stored.
            // The length is the converter's (its measuring pass); a pin is optional (a new artifact has none yet).
            let pipe = from_pipe.clone().expect("checked above");
            let len: u64 = a.need("--from-bytes")?.parse().map_err(|e| format!("--from-bytes: {e}"))?;
            let sha256 = a.one("--from-sha256")?.map(|h| parse_sha256(&h)).transpose()?;
            let name = a.one("--from-name")?.unwrap_or_else(|| "legacy".to_string());
            let mut reader: Box<dyn std::io::Read> = if pipe == "-" {
                Box::new(std::io::stdin().lock())
            } else {
                Box::new(std::fs::File::open(&pipe).map_err(|e| format!("{pipe}: {e}"))?)
            };
            let mut reader = std::io::BufReader::with_capacity(8 << 20, &mut reader);
            pack_legacy_stream_v3(LegacyStreamV3 { reader: &mut reader, len, name, sha256 }, &inputs, hook)?
        }
        (None, Some(url)) => {
            // The legacy file over HTTPS, read once in order and never stored: its SHA-256 must be the pinned LFS id before the index
            // is written. `curl` is the transport (no token is sent: a public file); a dropped connection resumes at the byte it reached.
            let len: u64 = a.need("--from-bytes")?.parse().map_err(|e| format!("--from-bytes: {e}"))?;
            let sha256 = parse_sha256(&a.need("--from-sha256")?)?;
            let name = a.one("--from-name")?.unwrap_or_else(|| url.rsplit('/').next().unwrap_or("legacy").to_string());
            let mut reader = CurlStream::new(url, len);
            pack_legacy_stream_v3(LegacyStreamV3 { reader: &mut reader, len, name, sha256: Some(sha256) }, &inputs, hook)?
        }
    };
    let w = &packed.weights;
    Ok(serde_json::json!({
        "root": hex(&packed.written.root),
        "index": packed.written.index_path.display().to_string(),
        "index_sha256": sha256_file(&packed.written.index_path)?,
        "data_bytes": packed.written.header.data_bytes,
        "segment_bytes": packed.written.header.segment_bytes,
        "segments": packed.written.header.segments.iter().enumerate().map(|(k, s)| serde_json::json!({
            "file": v3::segment_file_name_v3(&stem, k as u32),
            "bytes": s.bytes,
            "sha256": hex(&s.sha256),
        })).collect::<Vec<_>>(),
        "weights": {
            "legacy_format": w.legacy_format.name(),
            "section": v3::section_kind::name(w.section_kind),
            "inner_root_scheme": w.inner_root_scheme,
            "inner_root": hex(&w.inner_root),
            "inner_leaf_count": w.inner_leaf_count,
            "bytes": w.bytes,
        },
        "program": {
            "kind": packed.written.header.program.kind,
            "kernel_id": hex(&packed.written.header.program.kernel_id),
            "tokenizer_id": hex(&packed.written.header.program.tokenizer_id),
        },
    }))
}

fn parse_sha256(h: &str) -> Result<[u8; 32], String> {
    if h.len() != 64 || !h.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("--from-sha256: 64 hex digits".into());
    }
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&h[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// **A file read in order over HTTPS** by `curl` (`--location`, resolve → CDN), resumed with a `Range` from the byte reached when a
/// connection drops (at most a few times). Never sends a token. Ends exactly at the declared length.
struct CurlStream {
    url: String,
    len: u64,
    pos: u64,
    child: Option<std::process::Child>,
    retries_left: u32,
}

impl CurlStream {
    fn new(url: &str, len: u64) -> Self {
        Self { url: url.to_string(), len, pos: 0, child: None, retries_left: 8 }
    }

    fn spawn(&mut self) -> std::io::Result<()> {
        let child = std::process::Command::new("curl")
            .args(["--silent", "--show-error", "--location", "--fail", "--range", &format!("{}-", self.pos), "--", &self.url])
            .env_remove("HF_TOKEN")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()?;
        self.child = Some(child);
        Ok(())
    }

    fn drop_child(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl std::io::Read for CurlStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.len || buf.is_empty() {
            return Ok(0);
        }
        loop {
            if self.child.is_none() {
                self.spawn()?;
            }
            let want = buf.len().min((self.len - self.pos) as usize);
            let r = self.child.as_mut().and_then(|c| c.stdout.as_mut()).map(|o| o.read(&mut buf[..want]));
            match r {
                Some(Ok(n)) if n > 0 => {
                    self.pos += n as u64;
                    return Ok(n);
                }
                Some(Err(e)) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                other => {
                    // The stream ended or broke before the declared length: resume from here.
                    self.drop_child();
                    if self.retries_left == 0 {
                        return Err(std::io::Error::other(format!(
                            "{}: the stream ended at byte {} of {} ({other:?})",
                            self.url, self.pos, self.len
                        )));
                    }
                    self.retries_left -= 1;
                    eprintln!("palwart: {} interrupted at byte {}; resuming", self.url, self.pos);
                    std::thread::sleep(std::time::Duration::from_secs(5));
                }
            }
        }
    }
}

impl Drop for CurlStream {
    fn drop(&mut self) {
        self.drop_child();
    }
}

fn sha256_file(p: &Path) -> Result<String, String> {
    use sha2::{Digest as _, Sha256};
    let mut f = std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(hex(&h.finalize()))
}

fn run() -> Result<(serde_json::Value, bool), String> {
    let a = parse(std::env::args().skip(1).collect())?;
    let cmd = a.positional.first().cloned().ok_or(USAGE)?;
    let json = a.on("--json");
    let v = match cmd.as_str() {
        "pack" => pack(&a)?,
        "inspect" => {
            a.check_known(&["--data-dir"])?;
            summary_json_v3(&open(&a)?)
        }
        "verify" => {
            a.check_known(&["--data-dir"])?;
            verify_v3(&open(&a)?)?
        }
        "root" => {
            a.check_known(&["--data-dir"])?;
            let art = open(&a)?;
            serde_json::json!({ "root": art.root_hex() })
        }
        "sniff" => {
            let p = PathBuf::from(a.positional.get(1).ok_or("a file path is required")?);
            let f = v3::sniff_format_v3(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            serde_json::json!({ "file": p.display().to_string(), "format": f.name(), "legacy": f.is_legacy() })
        }
        "extract-legacy" => {
            a.check_known(&["--data-dir", "--out"])?;
            let art = open(&a)?;
            let out = PathBuf::from(a.need("--out")?);
            let f = extract_legacy_v3(&art, &out)?;
            serde_json::json!({ "out": out.display().to_string(), "format": f.name(), "sha256": sha256_file(&out)? })
        }
        _ => return Err(USAGE.into()),
    };
    Ok((v, json))
}

fn main() {
    match run() {
        Ok((v, json)) => {
            if json {
                println!("{}", serde_json::to_string(&v).unwrap_or_default());
            } else {
                println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            }
        }
        Err(e) => {
            eprintln!("palwart: {e}");
            std::process::exit(1);
        }
    }
}
