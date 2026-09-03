//! **ADR-0078 Decision 1, from the consumer's end: the bytes have to be a FILE.**
//!
//! *"The chain carries the derivation, never the artifact."* Everything else in this crate is
//! about the derivation being reproducible — the goldens, the two-architecture drill, the
//! `derived_id` arithmetic. All of it can hold while the thing a stranger recomputes is a blob no
//! reader on earth will open, and then Decision 1's promise is empty: the chain would be carrying
//! a receipt for a file that does not exist as a file.
//!
//! A hash match is not that proof. `artifact_hash` says two hosts produced the same bytes; it says
//! nothing about whether those bytes satisfy the invariants a MIDI sequencer or a glTF loader
//! relies on. Until this module landed, nothing in the tree had ever asked. The kinds' own unit
//! tests check the values they meant to write; a writer that is wrong about the format is wrong in
//! its tests the same way.
//!
//! **What an in-tree test can honestly claim.** There is no glTF, MIDI, PNG or STL library in this
//! workspace, and adding one to a consensus tree to check a test would be a bad trade. So the
//! validators below are written from the formats' own specifications — SMF 1.0, glTF 2.0 §4/§5,
//! RFC 2083 + RFC 1950/1951, and the binary STL layout — and they check the format's OWN
//! invariants: the chunk framing, the declared lengths against the real ones, the padding fill
//! bytes the spec names, the checksums, and every accessor's byte range inside its bufferView
//! inside its buffer. That is a STRUCTURAL check. It says the bytes satisfy what readers rely on;
//! it does not say that a particular reader accepted them, and this file does not pretend
//! otherwise. `scripts/misaka-palw-derived-proof.sh` carries the other half of that claim by
//! handing the same artifacts to tools that are not ours — `file(1)`'s magic tables, and Apple's
//! AudioToolbox `MusicSequenceFileLoad` — and reporting what they said.
//!
//! The validators are deliberately INDEPENDENT of the writers: their own CRC-32 and Adler-32
//! (pinned here against the standard vectors rather than borrowed from `crate::checksum`), their
//! own variable-length-quantity reader, their own chunk walker. A validator that shares code with
//! the writer agrees with it by construction and proves nothing.
//!
//! Every check runs over EVERY corpus sample of its kind, not a lucky one: a writer that is right
//! about a cube and wrong about a hierarchy is a writer that is wrong.

use kaspa_hashes::Hash64;
use misaka_palw_derive::{ClaimBinding, Derivation, derive_named};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// =============================================================================================
// deriving the corpus
// =============================================================================================

fn binding() -> ClaimBinding {
    ClaimBinding {
        network_domain: Hash64::from_bytes([0x01; 64]),
        claim_id: Hash64::from_bytes([0x02; 64]),
        output_root: Hash64::from_bytes([0x03; 64]),
        executor_pubkey: vec![0x11; 2592],
    }
}

fn corpus_dir(kind: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus").join(kind)
}

/// Every corpus answer of one kind directory, sorted, with `golden.json` left out — it is the pin,
/// not a sample. The same rule `palw-derive drill` uses, so the two speak about the same files.
fn corpus_answers(kind: &str) -> Vec<PathBuf> {
    let dir = corpus_dir(kind);
    let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json") && p.file_name().is_some_and(|n| n != "golden.json"))
        .collect();
    files.sort();
    files
}

/// Derive every corpus sample of one kind that this transformer accepts, and hand back the ones
/// that produced an artifact.
///
/// A refused sample is not a failure here: the `-refused-` and `9x-` rows exist to exercise the
/// walls, and `palw-derive drill` already pins WHICH wall each of them hits. What this returns is
/// the set of real artifacts, and it insists the set is not empty — a validator that silently
/// checked nothing is the failure mode this whole file exists to rule out.
fn derived(kind_dir: &str, transformer: &str) -> Vec<(String, Derivation)> {
    let binding = binding();
    let mut out = Vec::new();
    for path in corpus_answers(kind_dir) {
        let answer = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let leaf = path.file_name().unwrap().to_string_lossy().into_owned();
        match derive_named(transformer, &binding, &answer) {
            Ok(d) => out.push((leaf, d)),
            Err(e) => {
                assert!(e.is_refusal(), "{leaf}: {transformer} failed without refusing: {e}");
            }
        }
    }
    assert!(!out.is_empty(), "{transformer} derived nothing from corpus/{kind_dir} — this test would have checked no bytes at all");
    out
}

// =============================================================================================
// the validators' own checksums — pinned against the standard vectors, not borrowed
// =============================================================================================

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    crc ^ 0xFFFF_FFFF
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += x as u32;
            b += a;
        }
        a %= 65_521;
        b %= 65_521;
    }
    (b << 16) | a
}

/// The validators check the writers; nothing checks the validators, so their two checksums are
/// pinned against the vectors their own specifications publish (RFC 1950 §9 / the CRC-32 check
/// value of "123456789"). A validator whose CRC is wrong reports a corrupt file for a good one.
#[test]
fn the_validators_own_checksums_agree_with_the_published_vectors() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
    assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    assert_eq!(adler32(b""), 1);
}

// =============================================================================================
// a byte reader that refuses past the end instead of panicking
// =============================================================================================

struct Cur<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cur<'a> {
    fn new(b: &'a [u8]) -> Self {
        Cur { b, i: 0 }
    }
    fn left(&self) -> usize {
        self.b.len() - self.i
    }
    fn take(&mut self, n: usize, what: &str) -> Result<&'a [u8], String> {
        if self.left() < n {
            return Err(format!("{what}: wanted {n} bytes at offset {}, only {} remain", self.i, self.left()));
        }
        let s = &self.b[self.i..self.i + n];
        self.i += n;
        Ok(s)
    }
    fn u8(&mut self, what: &str) -> Result<u8, String> {
        Ok(self.take(1, what)?[0])
    }
    fn u16be(&mut self, what: &str) -> Result<u16, String> {
        let s = self.take(2, what)?;
        Ok(u16::from_be_bytes([s[0], s[1]]))
    }
    fn u32be(&mut self, what: &str) -> Result<u32, String> {
        let s = self.take(4, what)?;
        Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }
}

// =============================================================================================
// Standard MIDI File — SMF 1.0
// =============================================================================================

/// What a reader learns from a well-formed SMF, so a test can assert on it rather than on "no
/// error was returned".
#[derive(Debug)]
struct Smf {
    format: u16,
    ntrks: u16,
    /// Ticks per quarter note, or `None` for an SMPTE division.
    ppq: Option<u16>,
    /// Events per track, end-of-track included.
    events: Vec<usize>,
    /// Note-on events with a non-zero velocity, per track.
    note_ons: Vec<usize>,
}

/// A variable-length quantity (SMF 1.0, "Variable-length quantities"): seven bits per byte, high
/// bit set on every byte but the last, at most four bytes.
fn vlq(c: &mut Cur, what: &str) -> Result<u32, String> {
    let mut v: u32 = 0;
    for n in 0..4 {
        let b = c.u8(what)?;
        v = (v << 7) | (b & 0x7F) as u32;
        if b & 0x80 == 0 {
            return Ok(v);
        }
        if n == 3 {
            return Err(format!("{what}: a variable-length quantity ran past four bytes at offset {}", c.i));
        }
    }
    unreachable!()
}

fn parse_smf(bytes: &[u8]) -> Result<Smf, String> {
    let mut c = Cur::new(bytes);
    // ---- MThd -------------------------------------------------------------------------------
    if c.take(4, "header chunk type")? != b"MThd" {
        return Err("the file does not begin with an MThd chunk".into());
    }
    let hlen = c.u32be("header length")?;
    if hlen != 6 {
        return Err(format!("the MThd chunk declares {hlen} bytes; SMF 1.0 fixes it at 6"));
    }
    let format = c.u16be("format")?;
    if format > 2 {
        return Err(format!("format {format} is not one of SMF's 0, 1, 2"));
    }
    let ntrks = c.u16be("ntrks")?;
    if ntrks == 0 {
        return Err("the header declares zero tracks".into());
    }
    if format == 0 && ntrks != 1 {
        return Err(format!("format 0 declares {ntrks} tracks; a format-0 file holds exactly one"));
    }
    let division = c.u16be("division")?;
    let ppq = if division & 0x8000 == 0 {
        if division == 0 {
            return Err("a metrical division of 0 ticks per quarter note".into());
        }
        Some(division)
    } else {
        // SMPTE: the high byte is a negative frame rate, the low byte ticks per frame.
        let frames = -((division >> 8) as i8 as i32);
        let ticks = division & 0xFF;
        if !matches!(frames, 24 | 25 | 29 | 30) {
            return Err(format!("SMPTE division names {frames} frames per second"));
        }
        if ticks == 0 {
            return Err("an SMPTE division of 0 ticks per frame".into());
        }
        None
    };

    // ---- the tracks -------------------------------------------------------------------------
    let mut events = Vec::new();
    let mut note_ons = Vec::new();
    for t in 0..ntrks {
        if c.take(4, &format!("track {t} chunk type"))? != b"MTrk" {
            return Err(format!("track {t} does not begin with an MTrk chunk"));
        }
        let len = c.u32be(&format!("track {t} length"))? as usize;
        let body = c.take(len, &format!("track {t} body"))?.to_vec();
        let (n, ons) = parse_track(&body, t)?;
        events.push(n);
        note_ons.push(ons);
    }
    if c.left() != 0 {
        return Err(format!("{} bytes follow the last track chunk", c.left()));
    }
    Ok(Smf { format, ntrks, ppq, events, note_ons })
}

/// One MTrk body: delta time, event, repeat — with running status — ending in FF 2F 00 exactly at
/// the chunk boundary. "Ends in end-of-track" is the invariant every sequencer relies on to know
/// where a track stops; a file that runs off the end of its own chunk is the classic corrupt MIDI.
fn parse_track(body: &[u8], t: u16) -> Result<(usize, usize), String> {
    let mut c = Cur::new(body);
    let mut running: Option<u8> = None;
    let mut events = 0usize;
    let mut note_ons = 0usize;
    let mut ended = false;
    while c.left() > 0 {
        if ended {
            return Err(format!("track {t}: {} bytes follow the end-of-track event", c.left()));
        }
        let _delta = vlq(&mut c, &format!("track {t} delta time"))?;
        let first = c.u8(&format!("track {t} status"))?;
        let status = if first < 0x80 {
            // A data byte where a status could be: running status, and there must be one.
            let s = running.ok_or_else(|| format!("track {t}: a data byte at offset {} with no running status", c.i - 1))?;
            c.i -= 1;
            s
        } else {
            first
        };
        match status {
            0xFF => {
                // Meta. A meta event cancels running status (SMF 1.0).
                running = None;
                let ty = c.u8(&format!("track {t} meta type"))?;
                let len = vlq(&mut c, &format!("track {t} meta length"))? as usize;
                let data = c.take(len, &format!("track {t} meta data"))?;
                if ty == 0x2F {
                    if len != 0 {
                        return Err(format!("track {t}: end-of-track carries {len} bytes; FF 2F 00 carries none"));
                    }
                    ended = true;
                }
                if ty == 0x51 && len != 3 {
                    return Err(format!("track {t}: a set-tempo meta event of {len} bytes; it is three"));
                }
                if ty == 0x58 && len != 4 {
                    return Err(format!("track {t}: a time-signature meta event of {len} bytes; it is four"));
                }
                let _ = data;
            }
            0xF0 | 0xF7 => {
                running = None;
                let len = vlq(&mut c, &format!("track {t} sysex length"))? as usize;
                c.take(len, &format!("track {t} sysex data"))?;
            }
            s if s >= 0x80 => {
                running = Some(s);
                let n = match s & 0xF0 {
                    0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => 2,
                    0xC0 | 0xD0 => 1,
                    other => return Err(format!("track {t}: status byte {other:#04X} is not a channel message")),
                };
                let data = c.take(n, &format!("track {t} channel data"))?;
                for (i, &d) in data.iter().enumerate() {
                    if d >= 0x80 {
                        return Err(format!("track {t}: data byte {i} of a {s:#04X} event is {d:#04X}, which has its high bit set"));
                    }
                }
                if s & 0xF0 == 0x90 && data[1] != 0 {
                    note_ons += 1;
                }
            }
            other => return Err(format!("track {t}: {other:#04X} where a status byte belongs")),
        }
        events += 1;
    }
    if !ended {
        return Err(format!("track {t}: the chunk ends without an FF 2F 00 end-of-track event"));
    }
    Ok((events, note_ons))
}

/// **Every MIDI file the shipped corpus derives is a well-formed Standard MIDI File.**
///
/// The header's format/ntrks/division, one MTrk per declared track and no bytes after the last of
/// them, every delta time a legal variable-length quantity, every running-status run resolvable,
/// and every track ending in FF 2F 00 exactly at its own chunk boundary.
#[test]
fn every_derived_midi_file_is_a_well_formed_smf() {
    let mut checked = 0usize;
    for (leaf, d) in derived("music", "music/smf/v1") {
        let smf = parse_smf(&d.artifact.bytes).unwrap_or_else(|e| panic!("{leaf}: the derived MIDI is not a valid SMF: {e}"));
        assert_eq!(smf.format, 1, "{leaf}: the writer declares one track per part, which is format 1");
        assert_eq!(
            smf.ntrks as usize,
            smf.events.len(),
            "{leaf}: MThd declares {} tracks and the file holds {}",
            smf.ntrks,
            smf.events.len()
        );
        assert!(smf.ppq.is_some(), "{leaf}: the corpus is metrical; an SMPTE division would be a different file");
        // Track 0 is the tempo map (SMF 1.0's format-1 convention) and carries no notes.
        assert_eq!(smf.note_ons[0], 0, "{leaf}: track 0 is the tempo map and carries note-ons");
        // **The notes in the file are the notes the answer asked for, track by track.** Structural
        // validity alone is satisfied by an empty song; this is the check that says the artifact
        // is a rendering OF THIS DSL. `05-large-deltas` ships a part with no notes on purpose, so
        // "every part sounds" would be the wrong invariant — "every part sounds exactly what it
        // was given" is the right one.
        let dsl: serde_json::Value = serde_json::from_slice(&d.canonical_dsl).expect("the canonical DSL is JSON");
        let want: Vec<usize> =
            dsl["tracks"].as_array().expect("tracks").iter().map(|t| t["notes"].as_array().map(|n| n.len()).unwrap_or(0)).collect();
        assert_eq!(&smf.note_ons[1..], want.as_slice(), "{leaf}: the parts do not sound the DSL's notes");
        assert!(want.iter().sum::<usize>() > 0, "{leaf}: the corpus row asks for no notes at all");
        assert_eq!(d.artifact.extension, "mid");
        assert_eq!(d.artifact.media_type, "audio/midi");
        checked += 1;
    }
    assert!(checked >= 4, "only {checked} MIDI corpus rows were checked");
}

// =============================================================================================
// glTF 2.0 binary — GLB
// =============================================================================================

const GLB_MAGIC: u32 = 0x4654_6C67; // b"glTF" read little-endian
const GLB_JSON: u32 = 0x4E4F_534A; // "JSON"
const GLB_BIN: u32 = 0x004E_4942; // "BIN\0"

#[derive(Debug)]
struct Glb {
    json: serde_json::Value,
    json_chunk_len: usize,
    bin_len: usize,
    accessors: usize,
    /// The largest byte offset any accessor actually reads, so the test can say the BIN chunk is
    /// used and not merely present.
    high_water: usize,
}

fn u32le(s: &[u8]) -> u32 {
    u32::from_le_bytes([s[0], s[1], s[2], s[3]])
}

fn parse_glb(bytes: &[u8]) -> Result<Glb, String> {
    // ---- the 12-byte header (glTF 2.0 §4.4.1) ----------------------------------------------
    if bytes.len() < 12 {
        return Err(format!("{} bytes cannot hold a GLB header", bytes.len()));
    }
    let magic = u32le(&bytes[0..4]);
    if magic != GLB_MAGIC {
        return Err(format!("magic is {magic:#010X}, not glTF's {GLB_MAGIC:#010X}"));
    }
    let version = u32le(&bytes[4..8]);
    if version != 2 {
        return Err(format!("version {version}; this is glTF 2.0's container"));
    }
    let declared = u32le(&bytes[8..12]) as usize;
    if declared != bytes.len() {
        return Err(format!("the header declares {declared} bytes and the file is {}", bytes.len()));
    }
    if !bytes.len().is_multiple_of(4) {
        return Err(format!("a GLB is a sequence of 4-aligned chunks; {} bytes is not a multiple of 4", bytes.len()));
    }

    // ---- the chunks (§4.4.2) ----------------------------------------------------------------
    let mut off = 12usize;
    let mut chunks: Vec<(u32, &[u8])> = Vec::new();
    while off < bytes.len() {
        if bytes.len() - off < 8 {
            return Err(format!("{} trailing bytes at offset {off} cannot hold a chunk header", bytes.len() - off));
        }
        let len = u32le(&bytes[off..off + 4]) as usize;
        let ty = u32le(&bytes[off + 4..off + 8]);
        if !len.is_multiple_of(4) {
            return Err(format!("chunk at {off} declares {len} bytes; every chunk length is a multiple of 4"));
        }
        if bytes.len() - off - 8 < len {
            return Err(format!("chunk at {off} declares {len} bytes and only {} remain", bytes.len() - off - 8));
        }
        chunks.push((ty, &bytes[off + 8..off + 8 + len]));
        off += 8 + len;
    }
    if chunks.is_empty() {
        return Err("a GLB with no chunks".into());
    }
    if chunks[0].0 != GLB_JSON {
        return Err(format!("the first chunk is {:#010X}; §4.4.3.1 makes it JSON", chunks[0].0));
    }
    if chunks.iter().skip(1).any(|(t, _)| *t == GLB_JSON) {
        return Err("a second JSON chunk".into());
    }

    // The JSON chunk is padded with trailing SPACE (0x20) — §4.4.3.1's fill byte, and the reason
    // a loader can hand the chunk straight to a JSON parser. NUL padding here is the common
    // writer bug: most parsers reject it, and the ones that do not are being lenient.
    let json_bytes = chunks[0].1;
    let end = json_bytes.iter().rposition(|b| *b != 0x20).ok_or("the JSON chunk is all padding")?;
    let pad = json_bytes.len() - end - 1;
    if pad >= 4 {
        return Err(format!("the JSON chunk carries {pad} bytes of padding; alignment needs at most 3"));
    }
    for (i, b) in json_bytes[end + 1..].iter().enumerate() {
        if *b != 0x20 {
            return Err(format!("JSON padding byte {i} is {b:#04X}; §4.4.3.1 fills with 0x20"));
        }
    }
    let json: serde_json::Value =
        serde_json::from_slice(&json_bytes[..=end]).map_err(|e| format!("the JSON chunk does not parse: {e}"))?;

    // ---- the BIN chunk (§4.4.3.2) -----------------------------------------------------------
    let bin: &[u8] = match chunks.get(1) {
        Some((ty, data)) if *ty == GLB_BIN => data,
        Some((ty, _)) => return Err(format!("the second chunk is {ty:#010X}, neither BIN nor absent")),
        None => &[],
    };

    // ---- the document ------------------------------------------------------------------------
    let asset_version = json.pointer("/asset/version").and_then(|v| v.as_str()).ok_or("no asset.version")?;
    if asset_version != "2.0" {
        return Err(format!("asset.version is {asset_version:?}"));
    }
    let arr = |name: &str| -> Vec<serde_json::Value> { json.get(name).and_then(|v| v.as_array()).cloned().unwrap_or_default() };
    let buffers = arr("buffers");
    let views = arr("bufferViews");
    let accessors = arr("accessors");

    // Buffer 0 is the BIN chunk, and its declared length must sit inside the chunk with less than
    // four bytes of alignment padding after it (§4.4.3.2). This is the check that catches a
    // writer whose byte accounting drifted from what it actually wrote.
    if !buffers.is_empty() {
        let b0 = &buffers[0];
        if b0.get("uri").is_some() {
            return Err("buffer 0 names a uri; in a GLB it is the BIN chunk".into());
        }
        let declared = b0.get("byteLength").and_then(|v| v.as_u64()).ok_or("buffer 0 has no byteLength")? as usize;
        if declared > bin.len() {
            return Err(format!("buffer 0 declares {declared} bytes; the BIN chunk holds {}", bin.len()));
        }
        if bin.len() - declared >= 4 {
            return Err(format!(
                "the BIN chunk is {} bytes past buffer 0's {declared}; alignment allows at most 3",
                bin.len() - declared
            ));
        }
        for (i, b) in bin[declared..].iter().enumerate() {
            if *b != 0x00 {
                return Err(format!("BIN padding byte {i} is {b:#04X}; §4.4.3.2 fills with 0x00"));
            }
        }
    }
    for (i, b) in buffers.iter().enumerate().skip(1) {
        if b.get("uri").is_none() {
            return Err(format!("buffer {i} has no uri and is not the BIN chunk"));
        }
    }

    // Every bufferView inside its buffer.
    for (i, v) in views.iter().enumerate() {
        let buf = v.get("buffer").and_then(|x| x.as_u64()).ok_or(format!("bufferView {i} has no buffer"))? as usize;
        let blen = buffers
            .get(buf)
            .and_then(|b| b.get("byteLength"))
            .and_then(|x| x.as_u64())
            .ok_or(format!("bufferView {i} names buffer {buf}, which is not in the document"))? as usize;
        let off = v.get("byteOffset").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
        let len = v.get("byteLength").and_then(|x| x.as_u64()).ok_or(format!("bufferView {i} has no byteLength"))? as usize;
        if off + len > blen {
            return Err(format!("bufferView {i} runs to {} in a buffer of {blen}", off + len));
        }
        if let Some(stride) = v.get("byteStride").and_then(|x| x.as_u64())
            && (!(4..=252).contains(&stride) || !stride.is_multiple_of(4))
        {
            return Err(format!("bufferView {i} declares byteStride {stride}"));
        }
    }

    // **Every accessor's byte range inside its bufferView inside its buffer.** This is the check
    // a loader performs the instant before it reads, and the one an out-by-one in a writer's
    // offset arithmetic fails.
    let mut high_water = 0usize;
    for (i, a) in accessors.iter().enumerate() {
        let count = a.get("count").and_then(|x| x.as_u64()).ok_or(format!("accessor {i} has no count"))? as usize;
        if count == 0 {
            return Err(format!("accessor {i} has count 0; glTF requires at least one element"));
        }
        let ctype = a.get("componentType").and_then(|x| x.as_u64()).ok_or(format!("accessor {i} has no componentType"))?;
        let csize = match ctype {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            other => return Err(format!("accessor {i} names componentType {other}")),
        };
        let ty = a.get("type").and_then(|x| x.as_str()).ok_or(format!("accessor {i} has no type"))?;
        let ncomp = match ty {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            "MAT2" => 4,
            "MAT3" => 9,
            "MAT4" => 16,
            other => return Err(format!("accessor {i} names type {other:?}")),
        };
        let elem = csize * ncomp;
        let Some(view_ix) = a.get("bufferView").and_then(|x| x.as_u64()).map(|x| x as usize) else {
            // A bufferView-less accessor reads zeros; legal, and nothing to bound.
            continue;
        };
        let view = views.get(view_ix).ok_or(format!("accessor {i} names bufferView {view_ix}, which is not in the document"))?;
        let vlen = view.get("byteLength").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
        let voff = view.get("byteOffset").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
        let aoff = a.get("byteOffset").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
        let stride = view.get("byteStride").and_then(|x| x.as_u64()).map(|x| x as usize).unwrap_or(elem);
        if stride < elem {
            return Err(format!("accessor {i} reads {elem}-byte elements at a stride of {stride}"));
        }
        // §5.1.1: the offset of every element is a multiple of its component size.
        if !(voff + aoff).is_multiple_of(csize) {
            return Err(format!("accessor {i} starts at {} in the buffer, which is not {csize}-aligned", voff + aoff));
        }
        let last = aoff + (count - 1) * stride + elem;
        if last > vlen {
            return Err(format!("accessor {i} reads to {last} in a bufferView of {vlen}"));
        }
        high_water = high_water.max(voff + last);
    }

    // Meshes: an index accessor's component type and type are fixed by §5.24.
    for (mi, m) in arr("meshes").iter().enumerate() {
        let prims = m.get("primitives").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        if prims.is_empty() {
            return Err(format!("mesh {mi} has no primitives"));
        }
        for (pi, p) in prims.iter().enumerate() {
            let attrs = p.get("attributes").and_then(|v| v.as_object()).cloned().unwrap_or_default();
            if attrs.is_empty() {
                return Err(format!("mesh {mi} primitive {pi} has no attributes"));
            }
            if !attrs.contains_key("POSITION") {
                return Err(format!("mesh {mi} primitive {pi} has no POSITION attribute"));
            }
            for (name, v) in &attrs {
                let ix = v.as_u64().ok_or(format!("mesh {mi} primitive {pi} attribute {name} is not an index"))? as usize;
                if ix >= accessors.len() {
                    return Err(format!("mesh {mi} primitive {pi} attribute {name} names accessor {ix}"));
                }
            }
            if let Some(ix) = p.get("indices").and_then(|v| v.as_u64()) {
                let a = accessors.get(ix as usize).ok_or(format!("mesh {mi} primitive {pi} names index accessor {ix}"))?;
                let ct = a.get("componentType").and_then(|x| x.as_u64()).unwrap_or(0);
                if !matches!(ct, 5121 | 5123 | 5125) {
                    return Err(format!("mesh {mi} primitive {pi}'s indices have componentType {ct}"));
                }
                if a.get("type").and_then(|x| x.as_str()) != Some("SCALAR") {
                    return Err(format!("mesh {mi} primitive {pi}'s indices are not SCALAR"));
                }
                let count = a.get("count").and_then(|x| x.as_u64()).unwrap_or(0);
                let mode = p.get("mode").and_then(|x| x.as_u64()).unwrap_or(4);
                if mode == 4 && !count.is_multiple_of(3) {
                    return Err(format!("mesh {mi} primitive {pi} draws TRIANGLES from {count} indices"));
                }
            }
        }
    }

    // Nodes: every child index exists, no node is claimed by two parents, and no node is its own
    // ancestor — the three ways a scene graph stops being a graph a loader can walk.
    let nodes = arr("nodes");
    let mut claimed: BTreeSet<usize> = BTreeSet::new();
    for (i, n) in nodes.iter().enumerate() {
        for c in n.get("children").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
            let ix = c.as_u64().ok_or(format!("node {i} has a non-index child"))? as usize;
            if ix >= nodes.len() {
                return Err(format!("node {i} names child {ix} of {} nodes", nodes.len()));
            }
            if ix == i {
                return Err(format!("node {i} is its own child"));
            }
            if !claimed.insert(ix) {
                return Err(format!("node {ix} is a child of two parents"));
            }
        }
        if let Some(mix) = n.get("mesh").and_then(|v| v.as_u64())
            && mix as usize >= arr("meshes").len()
        {
            return Err(format!("node {i} names mesh {mix}"));
        }
    }
    for (si, s) in arr("scenes").iter().enumerate() {
        for r in s.get("nodes").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
            let ix = r.as_u64().ok_or(format!("scene {si} has a non-index root"))? as usize;
            if ix >= nodes.len() {
                return Err(format!("scene {si} names root node {ix}"));
            }
            if claimed.contains(&ix) {
                return Err(format!("scene {si}'s root node {ix} is also somebody's child"));
            }
        }
    }

    Ok(Glb { json, json_chunk_len: chunks[0].1.len(), bin_len: bin.len(), accessors: accessors.len(), high_water })
}

/// **Every GLB the shipped corpus derives is a well-formed glTF 2.0 binary.**
///
/// Magic, version, the declared total length against the real one, both chunk lengths with their
/// 4-byte alignment and the spec's own fill bytes (0x20 for JSON, 0x00 for BIN), the JSON parsing,
/// and every accessor's byte range inside its bufferView inside its buffer.
#[test]
fn every_derived_glb_is_a_well_formed_gltf_binary() {
    let mut checked = 0usize;
    for (leaf, d) in derived("scene", "scene/glb/v1") {
        let glb =
            parse_glb(&d.artifact.bytes).unwrap_or_else(|e| panic!("{leaf}: the derived GLB is not a valid glTF 2.0 binary: {e}"));
        assert!(glb.accessors > 0, "{leaf}: a scene with no accessors reads nothing");
        assert!(glb.bin_len > 0, "{leaf}: no BIN chunk, so the mesh data is nowhere");
        // The high-water mark says the BIN chunk is USED and not merely declared: a writer that
        // emits a buffer nothing reads passes every range check and produces an empty model.
        assert!(
            glb.high_water * 2 >= glb.bin_len,
            "{leaf}: accessors reach byte {} of a {}-byte BIN chunk — most of the buffer is unreachable",
            glb.high_water,
            glb.bin_len
        );
        assert!(glb.json.get("scenes").is_some(), "{leaf}: no scenes array, so a loader has nothing to show");
        assert_eq!(d.artifact.extension, "glb");
        assert_eq!(d.artifact.media_type, "model/gltf-binary");
        assert_eq!(12 + 8 + glb.json_chunk_len + 8 + glb.bin_len, d.artifact.bytes.len(), "{leaf}: the chunks do not tile the file");
        checked += 1;
    }
    assert!(checked >= 4, "only {checked} scene corpus rows were checked");
}

// =============================================================================================
// PNG — RFC 2083, with RFC 1950/1951 for the IDAT stream
// =============================================================================================

#[derive(Debug)]
struct Png {
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: u8,
    /// The inflated raw scanline stream, filter bytes included.
    raw_len: usize,
}

/// Inflate a zlib stream made of STORED deflate blocks. The writer declares
/// `png/1.2/rgba8-filter0-stored-v1`, so a Huffman block would itself be a finding — and refusing
/// by name is more useful here than carrying an inflater this test does not need.
fn inflate_stored(z: &[u8]) -> Result<Vec<u8>, String> {
    if z.len() < 6 {
        return Err(format!("a zlib stream of {} bytes", z.len()));
    }
    let (cmf, flg) = (z[0], z[1]);
    if cmf & 0x0F != 8 {
        return Err(format!("zlib CM is {}, not 8 (deflate)", cmf & 0x0F));
    }
    if flg & 0x20 != 0 {
        return Err("the zlib stream presets a dictionary".into());
    }
    if !(cmf as u16 * 256 + flg as u16).is_multiple_of(31) {
        return Err("the zlib header's FCHECK does not divide by 31".into());
    }
    let mut out = Vec::new();
    let mut i = 2usize;
    loop {
        if z.len() - i < 5 {
            return Err(format!("a deflate block header at {i} with {} bytes left", z.len() - i));
        }
        let hdr = z[i];
        let bfinal = hdr & 1;
        let btype = (hdr >> 1) & 3;
        if btype != 0 {
            return Err(format!("deflate block at {i} is BTYPE {btype}; this writer declares stored blocks only"));
        }
        let len = u16::from_le_bytes([z[i + 1], z[i + 2]]) as usize;
        let nlen = u16::from_le_bytes([z[i + 3], z[i + 4]]);
        if nlen != !(len as u16) {
            return Err(format!("stored block at {i}: NLEN is not the complement of LEN"));
        }
        i += 5;
        if z.len() - i < len {
            return Err(format!("stored block at {i} declares {len} bytes and {} remain", z.len() - i));
        }
        out.extend_from_slice(&z[i..i + len]);
        i += len;
        if bfinal == 1 {
            break;
        }
    }
    if z.len() - i != 4 {
        return Err(format!("{} bytes between the last deflate block and the end; the Adler-32 is four", z.len() - i));
    }
    let want = u32::from_be_bytes([z[i], z[i + 1], z[i + 2], z[i + 3]]);
    let got = adler32(&out);
    if want != got {
        return Err(format!("the zlib Adler-32 is {want:#010X}; the data hashes to {got:#010X}"));
    }
    Ok(out)
}

fn parse_png(bytes: &[u8]) -> Result<Png, String> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < 8 || bytes[..8] != SIG {
        return Err("the file does not begin with the PNG signature".into());
    }
    let mut c = Cur::new(&bytes[8..]);
    let mut ihdr: Option<Png> = None;
    let mut idat: Vec<u8> = Vec::new();
    let mut seen_iend = false;
    let mut idat_run_closed = false;
    let mut order: Vec<[u8; 4]> = Vec::new();
    while c.left() > 0 {
        if seen_iend {
            return Err(format!("{} bytes follow IEND", c.left()));
        }
        let len = c.u32be("chunk length")? as usize;
        let ty: [u8; 4] = c.take(4, "chunk type")?.try_into().unwrap();
        let data = c.take(len, "chunk data")?.to_vec();
        let want = c.u32be("chunk CRC")?;
        // RFC 2083 §3.2: the CRC is over the type AND the data.
        let mut preimage = ty.to_vec();
        preimage.extend_from_slice(&data);
        let got = crc32(&preimage);
        if want != got {
            return Err(format!("chunk {} has CRC {want:#010X}; its bytes hash to {got:#010X}", String::from_utf8_lossy(&ty)));
        }
        order.push(ty);
        match &ty {
            b"IHDR" => {
                if !order.is_empty() && order.len() != 1 {
                    return Err("IHDR is not the first chunk".into());
                }
                if len != 13 {
                    return Err(format!("IHDR carries {len} bytes; it is 13"));
                }
                let width = u32::from_be_bytes(data[0..4].try_into().unwrap());
                let height = u32::from_be_bytes(data[4..8].try_into().unwrap());
                let (bit_depth, color_type, comp, filt, interlace) = (data[8], data[9], data[10], data[11], data[12]);
                if width == 0 || height == 0 {
                    return Err(format!("IHDR is {width}x{height}"));
                }
                let ok = match color_type {
                    0 => matches!(bit_depth, 1 | 2 | 4 | 8 | 16),
                    3 => matches!(bit_depth, 1 | 2 | 4 | 8),
                    2 | 4 | 6 => matches!(bit_depth, 8 | 16),
                    other => return Err(format!("IHDR colour type {other}")),
                };
                if !ok {
                    return Err(format!("IHDR colour type {color_type} with bit depth {bit_depth}"));
                }
                if comp != 0 {
                    return Err(format!("IHDR compression method {comp}; PNG defines 0"));
                }
                if filt != 0 {
                    return Err(format!("IHDR filter method {filt}; PNG defines 0"));
                }
                if interlace != 0 {
                    return Err(format!("IHDR interlace {interlace}; this validator reads the non-interlaced layout"));
                }
                ihdr = Some(Png { width, height, bit_depth, color_type, raw_len: 0 });
            }
            b"IDAT" => {
                if idat_run_closed {
                    return Err("the IDAT chunks are not consecutive (RFC 2083 §4.1.3)".into());
                }
                idat.extend_from_slice(&data);
            }
            b"IEND" => {
                if len != 0 {
                    return Err(format!("IEND carries {len} bytes"));
                }
                seen_iend = true;
            }
            _ => {}
        }
        if ty != *b"IDAT" && !idat.is_empty() {
            idat_run_closed = true;
        }
    }
    if !seen_iend {
        return Err("the file ends without an IEND chunk".into());
    }
    let mut png = ihdr.ok_or("no IHDR chunk")?;
    if idat.is_empty() {
        return Err("no IDAT chunk, so the image has no pixels".into());
    }
    let raw = inflate_stored(&idat)?;
    // RFC 2083 §2.3: each scanline is a filter byte and then width*channels samples.
    let channels = match png.color_type {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => unreachable!(),
    };
    let stride = (png.width as usize * channels * png.bit_depth as usize).div_ceil(8);
    let want = png.height as usize * (1 + stride);
    if raw.len() != want {
        return Err(format!("the inflated stream is {} bytes; {}x{} needs {want}", raw.len(), png.width, png.height));
    }
    for y in 0..png.height as usize {
        let f = raw[y * (1 + stride)];
        if f > 4 {
            return Err(format!("scanline {y} names filter type {f}; PNG defines 0-4"));
        }
        if f != 0 {
            return Err(format!("scanline {y} uses filter {f}; the writer declares filter 0"));
        }
    }
    png.raw_len = raw.len();
    Ok(png)
}

/// **Every PNG the shipped corpus derives is a well-formed PNG.**
///
/// The signature, every chunk's CRC-32 recomputed over type and data, IHDR first and IEND last,
/// the IDAT run consecutive, the zlib stream's Adler-32, and an inflated stream that is exactly
/// `height * (1 + width*channels)` bytes with a legal filter byte on every scanline.
#[test]
fn every_derived_png_is_a_well_formed_png() {
    let mut checked = 0usize;
    for (leaf, d) in derived("image", "image/png/v1") {
        let png = parse_png(&d.artifact.bytes).unwrap_or_else(|e| panic!("{leaf}: the derived PNG is not a valid PNG: {e}"));
        assert_eq!(png.color_type, 6, "{leaf}: the writer declares RGBA");
        assert_eq!(png.bit_depth, 8, "{leaf}: the writer declares 8 bits per sample");
        assert_eq!(png.raw_len, png.height as usize * (1 + png.width as usize * 4));
        assert_eq!(d.artifact.extension, "png");
        assert_eq!(d.artifact.media_type, "image/png");
        checked += 1;
    }
    assert!(checked >= 4, "only {checked} image corpus rows were checked");
}

// =============================================================================================
// binary STL
// =============================================================================================

#[derive(Debug)]
struct Stl {
    triangles: usize,
    degenerate: usize,
}

fn f32le(s: &[u8]) -> f32 {
    f32::from_le_bytes([s[0], s[1], s[2], s[3]])
}

fn parse_stl(bytes: &[u8]) -> Result<Stl, String> {
    if bytes.len() < 84 {
        return Err(format!("{} bytes cannot hold an 80-byte header and a count", bytes.len()));
    }
    // A binary STL whose header begins with "solid" is sniffed as the ASCII format by most
    // readers; the layout permits it and the file is then unopenable in practice.
    if bytes.starts_with(b"solid") {
        return Err("the 80-byte header begins with \"solid\", which readers sniff as ASCII STL".into());
    }
    let count = u32le(&bytes[80..84]) as usize;
    let want = 84 + 50 * count;
    if bytes.len() != want {
        return Err(format!("the header declares {count} facets, which is {want} bytes; the file is {}", bytes.len()));
    }
    let mut degenerate = 0usize;
    for t in 0..count {
        let base = 84 + 50 * t;
        let mut v = [[0f32; 3]; 4];
        for (i, slot) in v.iter_mut().enumerate() {
            for (j, out) in slot.iter_mut().enumerate() {
                let x = f32le(&bytes[base + 12 * i + 4 * j..]);
                if !x.is_finite() {
                    return Err(format!("facet {t} carries a non-finite float ({x})"));
                }
                *out = x;
            }
        }
        // The writer declares `zero-normal`: the normal is left at (0,0,0) so that the artifact is
        // a function of the vertex set alone, and every reader recomputes it by the right-hand
        // rule. A non-zero normal here would be a second, redundant source of truth.
        if v[0] != [0.0, 0.0, 0.0] {
            return Err(format!("facet {t} carries a normal {:?}; the writer declares zero normals", v[0]));
        }
        if v[1] == v[2] || v[2] == v[3] || v[1] == v[3] {
            degenerate += 1;
        }
        let attr = u16::from_le_bytes([bytes[base + 48], bytes[base + 49]]);
        if attr != 0 {
            return Err(format!("facet {t} carries attribute byte count {attr}; the writer pins it to zero"));
        }
    }
    Ok(Stl { triangles: count, degenerate })
}

/// **Every STL the shipped corpus derives is a well-formed binary STL.**
///
/// The length equation `84 + 50n` against the declared facet count, finite floats throughout, the
/// attribute byte count pinned to zero, and no facet with two coincident vertices — a degenerate
/// triangle is a hole in a mesh a slicer will refuse.
#[test]
fn every_derived_stl_is_a_well_formed_binary_stl() {
    let mut checked = 0usize;
    for (leaf, d) in derived("cad", "cad/stl/v1") {
        let stl = parse_stl(&d.artifact.bytes).unwrap_or_else(|e| panic!("{leaf}: the derived STL is not a valid binary STL: {e}"));
        assert!(stl.triangles >= 4, "{leaf}: {} facets cannot enclose a volume", stl.triangles);
        assert_eq!(stl.degenerate, 0, "{leaf}: {} facets have two coincident vertices", stl.degenerate);
        assert_eq!(d.artifact.extension, "stl");
        assert_eq!(d.artifact.bytes.len(), 84 + 50 * stl.triangles);
        checked += 1;
    }
    assert!(checked >= 4, "only {checked} cad corpus rows were checked");
}

// =============================================================================================
// the validators are validators
// =============================================================================================

/// **A validator that cannot fail is not a check.**
///
/// Every one of the four is handed a file that is well-formed except for one thing, and has to
/// name that thing. Without this a writer regression would land beside a validator that had
/// quietly stopped looking, and the suite would stay green — which is the failure this whole file
/// was written to make impossible.
#[test]
fn each_validator_refuses_a_file_that_is_wrong_in_exactly_one_way() {
    let corrupt = |mut b: Vec<u8>, at: usize, to: u8| {
        b[at] = to;
        b
    };

    // MIDI: truncate the last track by one byte, so its end-of-track no longer lands on the chunk
    // boundary. The bytes are still a plausible MIDI file to a sniffer.
    let midi = derived("music", "music/smf/v1").remove(0).1.artifact.bytes;
    assert!(parse_smf(&midi).is_ok());
    let mut short = midi.clone();
    short.pop();
    let e = parse_smf(&short).expect_err("a truncated track parsed");
    assert!(e.contains("track"), "the SMF validator's refusal does not name a track: {e}");
    // and a header that lies about how many tracks follow
    let e = parse_smf(&corrupt(midi.clone(), 11, 9)).expect_err("a wrong ntrks parsed");
    assert!(e.contains("track"), "{e}");

    // GLB: fill the JSON chunk's alignment padding with NUL instead of the spec's 0x20. Most of
    // the file is untouched and the JSON still parses — this is exactly the writer bug the fill
    // byte check exists for.
    // Not just any corpus GLB: one whose JSON chunk actually NEEDED alignment padding, or the
    // mutation would have nothing to corrupt and the check would pass by not checking.
    let scenes = derived("scene", "scene/glb/v1");
    let (pad_leaf, glb, pad_at) = scenes
        .iter()
        .find_map(|(leaf, d)| {
            let b = &d.artifact.bytes;
            let g = parse_glb(b).unwrap_or_else(|e| panic!("{leaf}: {e}"));
            let json_end = 12 + 8 + g.json_chunk_len;
            (12 + 8..json_end).rev().find(|i| b[*i] == 0x20).map(|i| (leaf.clone(), b.clone(), i))
        })
        .expect("no corpus GLB's JSON chunk needed alignment padding, so the fill-byte check is untested");
    let mut nul_pad = glb.clone();
    nul_pad[pad_at] = 0x00;
    let e = parse_glb(&nul_pad).expect_err("NUL padding in the JSON chunk parsed");
    assert!(e.contains("0x20") || e.contains("does not parse"), "{pad_leaf}: {e}");
    // and a declared total length that is not the file's
    let mut wrong_len = glb.clone();
    wrong_len[8] = wrong_len[8].wrapping_add(4);
    let e = parse_glb(&wrong_len).expect_err("a wrong total length parsed");
    assert!(e.contains("declares"), "{e}");

    // PNG: flip one byte of pixel data. Every chunk boundary is intact; only the CRC moves.
    let png = derived("image", "image/png/v1").remove(0).1.artifact.bytes;
    assert!(parse_png(&png).is_ok());
    let flip = png.len() - 20;
    let e = parse_png(&corrupt(png.clone(), flip, png[flip] ^ 0xFF)).expect_err("a flipped byte parsed");
    assert!(e.contains("CRC") || e.contains("Adler"), "the PNG validator's refusal names neither checksum: {e}");

    // STL: claim one more facet than the file carries.
    let stl = derived("cad", "cad/stl/v1").remove(0).1.artifact.bytes;
    assert!(parse_stl(&stl).is_ok());
    let mut more = stl.clone();
    more[80] = more[80].wrapping_add(1);
    let e = parse_stl(&more).expect_err("a wrong facet count parsed");
    assert!(e.contains("facets"), "{e}");
}
