//! Kind `music` (ADR-0078 Decision 8): the note-list DSL — pitch, onset and duration in ticks,
//! channels, programs — and the Standard MIDI File writer that makes a `.mid` of it. The row's
//! determinism basis is "SMF bytes are canonical", and this module makes that literally true:
//! one event order, one delta-time spelling, one status byte per event (no running status), so
//! the artifact is a pure function of the canonical DSL in integer arithmetic only (Decision 3,
//! X3). Waveform synthesis is not covered, by the row's own words.
//!
//! The grammar `music/v1` is JSON with exactly these keys:
//!
//! ```text
//! { "v": 1,
//!   "ppq": 96 | 192 | 480 | 960,                      ticks per quarter note
//!   "tempo_us_per_quarter": 1..=16777215,             microseconds per quarter note
//!   "time_signature": [numerator 1..=32, denominator 1 | 2 | 4 | 8 | 16 | 32],
//!   "tracks": [ { "name": string (0..=64 bytes), "channel": 0..=15, "program": 0..=127,
//!                 "notes": [ { "pitch": 0..=127, "velocity": 1..=127,
//!                              "onset": 0..=2^28-1, "duration": 1..=2^28-1 } ] } ] }
//! ```
//!
//! 1..=64 tracks, at most 65 536 notes in all, and every `onset + duration` at most 2^28.
//! Canonicalization is `canon_json`'s (sorted keys, no whitespace, integers only) plus this
//! schema; it reorders nothing (Decision 2: nothing semantic). The order of the notes inside an
//! answer is the answer's; it is the WRITER that sorts events, so two answers listing the same
//! notes in different orders have different `dsl_hash`es and one `artifact_hash`.
//!
//! The artifact is SMF format 1: `MThd` (format 1, `1 + tracks` chunks, division = ppq). Track 0
//! is the tempo track — a track name ("tempo"), the tempo, the time signature and end of track,
//! all at delta 0. Each DSL track is one `MTrk`: its name, a program change, then every note as
//! a note-on at `onset` and a note-off (velocity 0x40) at `onset + duration`, the events sorted
//! by `(tick, class, pitch, velocity)` with note-off (class 0) before note-on (class 1) at the
//! same tick, then end of track. Delta times are MIDI variable-length quantities.

use crate::bytes::{put_u16_be, put_u32_be};
use crate::canon_json::{CanonValue, parse_canonical, write_canonical};
use crate::{Artifact, DeriveError, Discipline, Grammar, Transformer, TransformerManifest};
use kaspa_consensus_core::palw_derived_v1::kind;
use std::collections::BTreeMap;

/// The grammar's name; its id is `H(domain ‖ name)` (`ids::grammar_id_v1`).
pub const GRAMMAR_NAME: &str = "music/v1";
/// The transformer's name.
pub const TRANSFORMER_NAME: &str = "music/smf/v1";
/// The canonical writer the manifest names.
pub const WRITER_NAME: &str = "standard-midi-file/1.0/canonical-v1";
/// The artifact's media type and file extension.
pub const MEDIA_TYPE: &str = "audio/midi";
pub const EXTENSION: &str = "mid";

/// The divisions (ticks per quarter note) the grammar admits.
pub const PPQ_ALLOWED: [i128; 4] = [96, 192, 480, 960];
/// The tempo meta event spells the tempo in three bytes; this is the largest it can hold.
pub const TEMPO_US_PER_QUARTER_MAX: i128 = 0xFF_FFFF;
pub const TIME_SIGNATURE_NUMERATOR_MAX: i128 = 32;
/// The denominators the time-signature meta event can spell (it stores `log2`).
pub const TIME_SIGNATURE_DENOMINATORS: [i128; 6] = [1, 2, 4, 8, 16, 32];
pub const TRACKS_MAX: usize = 64;
pub const NOTES_MAX_TOTAL: usize = 65_536;
pub const TRACK_NAME_MAX_BYTES: usize = 64;
/// No note ends after this tick; every onset and every duration is below it.
pub const TICK_END_MAX: i128 = 1 << 28;
/// The largest artifact the transformer hands out. The schema's bounds keep every artifact
/// under 1 MiB; the ceiling is stated so that the bound is a number and not an accident.
pub const ARTIFACT_MAX_BYTES: usize = 16 << 20;
/// The largest value a MIDI variable-length quantity spells (four bytes). Every delta the
/// writer emits is at most `TICK_END_MAX - 1`, which is exactly this.
pub const VLQ_MAX: u32 = 0x0FFF_FFFF;
/// The velocity every note-off carries.
pub const NOTE_OFF_VELOCITY: u8 = 0x40;

const META_TRACK_NAME: u8 = 0x03;
const META_END_OF_TRACK: u8 = 0x2F;
const META_TEMPO: u8 = 0x51;
const META_TIME_SIGNATURE: u8 = 0x58;
/// MIDI clocks per metronome click, and 32nd notes per quarter note: the two fixed bytes of
/// the time-signature meta event.
const TIME_SIGNATURE_CLOCKS_PER_CLICK: u8 = 24;
const TIME_SIGNATURE_32NDS_PER_QUARTER: u8 = 8;
const STATUS_NOTE_OFF: u8 = 0x80;
const STATUS_NOTE_ON: u8 = 0x90;
const STATUS_PROGRAM_CHANGE: u8 = 0xC0;
/// The event classes of the canonical sort: a note-off sorts before a note-on at one tick.
pub const EVENT_NOTE_OFF: u8 = 0;
pub const EVENT_NOTE_ON: u8 = 1;

/// The grammar `music/v1`.
pub struct MusicGrammar;
/// The transformer `music/smf/v1`: canonical `music/v1` bytes to a Standard MIDI File.
pub struct MusicSmfTransformer;

/// This kind's grammar and transformer, as the registry sees them.
pub fn register() -> (Vec<Box<dyn Grammar>>, Vec<Box<dyn Transformer>>) {
    (vec![Box::new(MusicGrammar)], vec![Box::new(MusicSmfTransformer)])
}

impl Grammar for MusicGrammar {
    fn name(&self) -> &'static str {
        GRAMMAR_NAME
    }

    /// Parse, hold to the schema, re-emit. A refusal anywhere is `DeriveError::Grammar` (X4).
    fn canonicalize(&self, answer: &[u8]) -> Result<Vec<u8>, DeriveError> {
        let value = parse_canonical(answer)?;
        parse_song(&value)?;
        Ok(write_canonical(&value))
    }
}

impl Transformer for MusicSmfTransformer {
    fn manifest(&self) -> TransformerManifest {
        TransformerManifest {
            name: TRANSFORMER_NAME,
            kind: kind::MUSIC,
            grammar: GRAMMAR_NAME,
            discipline: Discipline::Integer,
            writer: WRITER_NAME,
            source_tree_sha256: crate::SOURCE_TREE_SHA256_HEX,
        }
    }

    fn run(&self, dsl: &[u8]) -> Result<Artifact, DeriveError> {
        let song = canonical_song(dsl)?;
        let bytes = write_smf(&song)?;
        Ok(Artifact { bytes, media_type: MEDIA_TYPE, extension: EXTENSION })
    }
}

/// One note: MIDI pitch, note-on velocity, onset and duration in ticks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub pitch: u8,
    pub velocity: u8,
    pub onset: u32,
    pub duration: u32,
}

/// One DSL track: one `MTrk` on one channel under one program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub name: String,
    pub channel: u8,
    pub program: u8,
    pub notes: Vec<Note>,
}

/// A whole answer, validated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Song {
    pub ppq: u16,
    pub tempo_us_per_quarter: u32,
    pub numerator: u8,
    pub denominator: u8,
    pub tracks: Vec<Track>,
}

fn grammar(msg: String) -> DeriveError {
    DeriveError::Grammar(msg)
}

fn object<'a>(v: &'a CanonValue, what: &str) -> Result<&'a BTreeMap<String, CanonValue>, DeriveError> {
    v.as_obj().ok_or_else(|| grammar(format!("{what} is not an object")))
}

/// Exactly `expected` keys: an unknown key and a missing key are each a refusal by name.
fn exact_keys(obj: &BTreeMap<String, CanonValue>, expected: &[&str], what: &str) -> Result<(), DeriveError> {
    for key in obj.keys() {
        if !expected.contains(&key.as_str()) {
            return Err(grammar(format!("{what}: unknown key {key:?}")));
        }
    }
    for key in expected {
        if !obj.contains_key(*key) {
            return Err(grammar(format!("{what}: missing key {key:?}")));
        }
    }
    Ok(())
}

fn integer(v: &CanonValue, what: &str) -> Result<i128, DeriveError> {
    match v {
        CanonValue::Int(i) => Ok(*i),
        _ => Err(grammar(format!("{what} must be an integer"))),
    }
}

fn integer_in(v: &CanonValue, lo: i128, hi: i128, what: &str) -> Result<i128, DeriveError> {
    let i = integer(v, what)?;
    if (lo..=hi).contains(&i) { Ok(i) } else { Err(grammar(format!("{what} {i} is outside {lo}..={hi}"))) }
}

fn integer_one_of(v: &CanonValue, allowed: &[i128], what: &str) -> Result<i128, DeriveError> {
    let i = integer(v, what)?;
    if allowed.contains(&i) {
        Ok(i)
    } else {
        let list = allowed.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ");
        Err(grammar(format!("{what} {i} is not one of {list}")))
    }
}

/// Hold a parsed answer to the `music/v1` schema and lift it into a [`Song`]. Every violation
/// is `DeriveError::Grammar` and names what it saw.
pub fn parse_song(v: &CanonValue) -> Result<Song, DeriveError> {
    let top = object(v, "top level")?;
    exact_keys(top, &["v", "ppq", "tempo_us_per_quarter", "time_signature", "tracks"], "top level")?;
    let version = integer(&top["v"], "v")?;
    if version != 1 {
        return Err(grammar(format!("v must be 1, not {version}")));
    }
    let ppq = integer_one_of(&top["ppq"], &PPQ_ALLOWED, "ppq")? as u16;
    let tempo_us_per_quarter = integer_in(&top["tempo_us_per_quarter"], 1, TEMPO_US_PER_QUARTER_MAX, "tempo_us_per_quarter")? as u32;
    let time_signature = top["time_signature"].as_arr().ok_or_else(|| grammar("time_signature must be an array".into()))?;
    if time_signature.len() != 2 {
        return Err(grammar(format!("time_signature must be [numerator, denominator], not {} items", time_signature.len())));
    }
    let numerator = integer_in(&time_signature[0], 1, TIME_SIGNATURE_NUMERATOR_MAX, "time_signature numerator")? as u8;
    let denominator = integer_one_of(&time_signature[1], &TIME_SIGNATURE_DENOMINATORS, "time_signature denominator")? as u8;
    let tracks_in = top["tracks"].as_arr().ok_or_else(|| grammar("tracks must be an array".into()))?;
    if tracks_in.is_empty() || tracks_in.len() > TRACKS_MAX {
        return Err(grammar(format!("tracks must hold 1..={TRACKS_MAX} tracks, not {}", tracks_in.len())));
    }
    let mut tracks = Vec::with_capacity(tracks_in.len());
    let mut notes_total = 0usize;
    for (ti, tv) in tracks_in.iter().enumerate() {
        let what = format!("track {ti}");
        let t = object(tv, &what)?;
        exact_keys(t, &["name", "channel", "program", "notes"], &what)?;
        let name = t["name"].as_str().ok_or_else(|| grammar(format!("{what} name must be a string")))?;
        if name.len() > TRACK_NAME_MAX_BYTES {
            return Err(grammar(format!("{what} name is {} bytes; at most {TRACK_NAME_MAX_BYTES}", name.len())));
        }
        let channel = integer_in(&t["channel"], 0, 15, &format!("{what} channel"))? as u8;
        let program = integer_in(&t["program"], 0, 127, &format!("{what} program"))? as u8;
        let notes_in = t["notes"].as_arr().ok_or_else(|| grammar(format!("{what} notes must be an array")))?;
        notes_total += notes_in.len();
        if notes_total > NOTES_MAX_TOTAL {
            return Err(grammar(format!("more than {NOTES_MAX_TOTAL} notes in all")));
        }
        let mut notes = Vec::with_capacity(notes_in.len());
        for (ni, nv) in notes_in.iter().enumerate() {
            let what = format!("track {ti} note {ni}");
            let n = object(nv, &what)?;
            exact_keys(n, &["pitch", "velocity", "onset", "duration"], &what)?;
            let pitch = integer_in(&n["pitch"], 0, 127, &format!("{what} pitch"))? as u8;
            let velocity = integer_in(&n["velocity"], 1, 127, &format!("{what} velocity"))? as u8;
            let onset = integer_in(&n["onset"], 0, TICK_END_MAX - 1, &format!("{what} onset"))?;
            let duration = integer_in(&n["duration"], 1, TICK_END_MAX - 1, &format!("{what} duration"))?;
            let end = onset + duration;
            if end > TICK_END_MAX {
                return Err(grammar(format!("{what} ends at tick {end}, past {TICK_END_MAX}")));
            }
            notes.push(Note { pitch, velocity, onset: onset as u32, duration: duration as u32 });
        }
        tracks.push(Track { name: name.to_string(), channel, program, notes });
    }
    Ok(Song { ppq, tempo_us_per_quarter, numerator, denominator, tracks })
}

/// The song behind canonical `music/v1` bytes. The transformer repairs nothing: input that is
/// not exactly the grammar's own output — unparseable, off-schema, or merely spelled
/// differently — is refused as `DeriveError::Transformer`.
pub fn canonical_song(dsl: &[u8]) -> Result<Song, DeriveError> {
    let not_canonical = |e: DeriveError| DeriveError::Transformer(format!("input is not canonical music/v1: {e}"));
    let value = parse_canonical(dsl).map_err(not_canonical)?;
    let song = parse_song(&value).map_err(not_canonical)?;
    if write_canonical(&value) != dsl {
        return Err(DeriveError::Transformer("input is not canonical music/v1: the bytes differ from their canonical form".into()));
    }
    Ok(song)
}

/// A MIDI variable-length quantity: 7 bits per byte, most significant group first, bit 7 set
/// on every byte but the last. Any `u32` is spelled; the writer keeps its deltas at or below
/// [`VLQ_MAX`], the four-byte ceiling the format states.
pub fn encode_vlq(value: u32, out: &mut Vec<u8>) {
    let mut groups: Vec<u8> = Vec::with_capacity(5);
    let mut v = value;
    loop {
        groups.push((v & 0x7F) as u8);
        v >>= 7;
        if v == 0 {
            break;
        }
    }
    while let Some(group) = groups.pop() {
        out.push(if groups.is_empty() { group } else { group | 0x80 });
    }
}

fn delta_time(out: &mut Vec<u8>, delta: u32) -> Result<(), DeriveError> {
    if delta > VLQ_MAX {
        return Err(DeriveError::Transformer(format!("delta time {delta} exceeds the variable-length quantity ceiling {VLQ_MAX}")));
    }
    encode_vlq(delta, out);
    Ok(())
}

/// A meta event at delta 0: `00 FF <type> <length> <data>`. Every meta event this writer
/// emits is at delta 0 (names, tempo and time signature open a track; end of track follows
/// the last event with no gap).
fn meta(out: &mut Vec<u8>, ty: u8, data: &[u8]) {
    out.push(0x00);
    out.push(0xFF);
    out.push(ty);
    encode_vlq(data.len() as u32, out);
    out.extend_from_slice(data);
}

/// One event of a track's canonical order: `(tick, class, pitch, velocity)`. Tuple order is
/// the sort order, so a note-off ([`EVENT_NOTE_OFF`]) precedes a note-on at the same tick, and
/// two events equal in every field are the same bytes in either order.
pub type Event = (u32, u8, u8, u8);

/// A track's note events in canonical order, whatever order the notes were listed in.
pub fn track_events(track: &Track) -> Vec<Event> {
    let mut events = Vec::with_capacity(track.notes.len() * 2);
    for n in &track.notes {
        events.push((n.onset, EVENT_NOTE_ON, n.pitch, n.velocity));
        events.push((n.onset + n.duration, EVENT_NOTE_OFF, n.pitch, NOTE_OFF_VELOCITY));
    }
    events.sort();
    events
}

fn tempo_track(song: &Song) -> Vec<u8> {
    let mut body = Vec::new();
    meta(&mut body, META_TRACK_NAME, b"tempo");
    let t = song.tempo_us_per_quarter;
    meta(&mut body, META_TEMPO, &[(t >> 16) as u8, (t >> 8) as u8, t as u8]);
    let dd = song.denominator.trailing_zeros() as u8;
    meta(&mut body, META_TIME_SIGNATURE, &[song.numerator, dd, TIME_SIGNATURE_CLOCKS_PER_CLICK, TIME_SIGNATURE_32NDS_PER_QUARTER]);
    meta(&mut body, META_END_OF_TRACK, &[]);
    body
}

fn note_track(track: &Track) -> Result<Vec<u8>, DeriveError> {
    let mut body = Vec::with_capacity(16 + track.name.len() + track.notes.len() * 14);
    meta(&mut body, META_TRACK_NAME, track.name.as_bytes());
    body.push(0x00);
    body.push(STATUS_PROGRAM_CHANGE | track.channel);
    body.push(track.program);
    let mut previous = 0u32;
    for (tick, class, pitch, velocity) in track_events(track) {
        delta_time(&mut body, tick - previous)?;
        previous = tick;
        let status = if class == EVENT_NOTE_OFF { STATUS_NOTE_OFF } else { STATUS_NOTE_ON };
        body.push(status | track.channel);
        body.push(pitch);
        body.push(velocity);
    }
    meta(&mut body, META_END_OF_TRACK, &[]);
    Ok(body)
}

fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(tag);
    put_u32_be(out, body.len() as u32);
    out.extend_from_slice(body);
}

/// The canonical Standard MIDI File of a song (format 1).
pub fn write_smf(song: &Song) -> Result<Vec<u8>, DeriveError> {
    let mut out = Vec::new();
    out.extend_from_slice(b"MThd");
    put_u32_be(&mut out, 6);
    put_u16_be(&mut out, 1);
    put_u16_be(&mut out, (1 + song.tracks.len()) as u16);
    put_u16_be(&mut out, song.ppq);
    chunk(&mut out, b"MTrk", &tempo_track(song));
    for track in &song.tracks {
        chunk(&mut out, b"MTrk", &note_track(track)?);
    }
    check_artifact_size(out.len())?;
    Ok(out)
}

fn check_artifact_size(len: usize) -> Result<(), DeriveError> {
    if len > ARTIFACT_MAX_BYTES {
        return Err(DeriveError::Transformer(format!("artifact is {len} bytes; at most {ARTIFACT_MAX_BYTES}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derive::{ClaimBinding, derive_named, derive_with};
    use crate::ids::{artifact_hash_v1, dsl_hash_v1, grammar_id_v1, transformer_id};
    use kaspa_consensus_core::palw_derived_v1::PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN;
    use kaspa_hashes::Hash64;
    use std::path::{Path, PathBuf};

    const SINGLE: &str = r#"{
        "v": 1, "ppq": 480, "tempo_us_per_quarter": 500000, "time_signature": [4, 4],
        "tracks": [ { "name": "lead", "channel": 0, "program": 0,
                      "notes": [ { "pitch": 60, "velocity": 100, "onset": 0, "duration": 480 } ] } ]
    }"#;

    /// Two tracks; the chords are listed out of pitch order on purpose, and every bar line has
    /// a note-off and a note-on on the same tick.
    const CHORDS: &str = r#"{
        "v": 1, "ppq": 96, "tempo_us_per_quarter": 600000, "time_signature": [4, 4],
        "tracks": [
          { "name": "piano", "channel": 0, "program": 0, "notes": [
              { "pitch": 67, "velocity": 96, "onset": 0, "duration": 384 },
              { "pitch": 60, "velocity": 96, "onset": 0, "duration": 384 },
              { "pitch": 64, "velocity": 96, "onset": 0, "duration": 384 },
              { "pitch": 64, "velocity": 96, "onset": 384, "duration": 384 },
              { "pitch": 57, "velocity": 96, "onset": 384, "duration": 384 },
              { "pitch": 60, "velocity": 96, "onset": 384, "duration": 384 } ] },
          { "name": "bass", "channel": 1, "program": 33, "notes": [
              { "pitch": 36, "velocity": 80, "onset": 192, "duration": 192 },
              { "pitch": 36, "velocity": 80, "onset": 0, "duration": 192 },
              { "pitch": 33, "velocity": 80, "onset": 384, "duration": 192 } ] } ]
    }"#;

    fn binding() -> ClaimBinding {
        ClaimBinding {
            network_domain: Hash64::from_bytes([1u8; 64]),
            claim_id: Hash64::from_bytes([2u8; 64]),
            output_root: Hash64::from_bytes([3u8; 64]),
            executor_pubkey: vec![7u8; PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN],
        }
    }

    fn canonical(answer: &str) -> Vec<u8> {
        MusicGrammar.canonicalize(answer.as_bytes()).unwrap()
    }

    fn artifact(answer: &str) -> Vec<u8> {
        MusicSmfTransformer.run(&canonical(answer)).unwrap().bytes
    }

    fn song(answer: &str) -> Song {
        parse_song(&parse_canonical(answer.as_bytes()).unwrap()).unwrap()
    }

    #[track_caller]
    fn refused(answer: &str, fragment: &str) {
        match MusicGrammar.canonicalize(answer.as_bytes()) {
            Err(DeriveError::Grammar(msg)) => {
                assert!(msg.contains(fragment), "refusal {msg:?} does not mention {fragment:?}")
            }
            other => panic!("expected a grammar refusal mentioning {fragment:?}, got {other:?}"),
        }
    }

    /// `SINGLE` with one substring replaced — the way every refusal below is built.
    fn single_with(from: &str, to: &str) -> String {
        assert!(SINGLE.contains(from), "{from:?} is not in the sample");
        SINGLE.replacen(from, to, 1)
    }

    // ---- (1) canonicalization ------------------------------------------------------------

    #[test]
    fn canonical_form_sorts_keys_strips_whitespace_and_is_idempotent() {
        let once = canonical(SINGLE);
        let expected = br#"{"ppq":480,"tempo_us_per_quarter":500000,"time_signature":[4,4],"tracks":[{"channel":0,"name":"lead","notes":[{"duration":480,"onset":0,"pitch":60,"velocity":100}],"program":0}],"v":1}"#;
        assert_eq!(once, expected);
        let twice = MusicGrammar.canonicalize(&once).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn registration_and_manifest() {
        let (grammars, transformers) = register();
        assert_eq!(grammars.len(), 1);
        assert_eq!(transformers.len(), 1);
        assert_eq!(grammars[0].name(), "music/v1");
        let m = transformers[0].manifest();
        assert_eq!(m.name, "music/smf/v1");
        assert_eq!(m.kind, kind::MUSIC);
        assert_eq!(m.kind, 6);
        assert_eq!(m.grammar, "music/v1");
        assert_eq!(m.discipline, Discipline::Integer);
        assert_eq!(m.writer, "standard-midi-file/1.0/canonical-v1");
        assert_eq!(m.source_tree_sha256, crate::SOURCE_TREE_SHA256_HEX);
        assert!(crate::registry::transformer_by_name("music/smf/v1").is_some());
        assert!(crate::registry::grammar_by_name("music/v1").is_some());
        assert!(crate::registry::transformer_by_id(&transformer_id(&m)).is_some());
        let a = MusicSmfTransformer.run(&canonical(SINGLE)).unwrap();
        assert_eq!((a.media_type, a.extension), ("audio/midi", "mid"));
    }

    // ---- (2) every schema refusal ----------------------------------------------------------

    #[test]
    fn refuses_what_is_not_the_schema() {
        refused("[1]", "top level is not an object");
        refused("{", "json");
        refused(r#"{"v":1.0}"#, "non-integer");
        refused(&single_with(r#""v": 1,"#, r#""v": 1, "extra": 1,"#), "unknown key \"extra\"");
        refused(&single_with(r#""ppq": 480,"#, ""), "missing key \"ppq\"");
        refused(&single_with(r#""v": 1,"#, r#""v": 2,"#), "v must be 1");
        refused(&single_with(r#""v": 1,"#, r#""v": "1","#), "v must be an integer");
        refused(&single_with(r#""ppq": 480,"#, r#""ppq": 500,"#), "ppq 500 is not one of 96, 192, 480, 960");
        refused(&single_with("500000", "0"), "tempo_us_per_quarter 0 is outside 1..=16777215");
        refused(&single_with("500000", "16777216"), "tempo_us_per_quarter 16777216 is outside");
        refused(&single_with("[4, 4]", "4"), "time_signature must be an array");
        refused(&single_with("[4, 4]", "[4, 4, 4]"), "time_signature must be [numerator, denominator]");
        refused(&single_with("[4, 4]", "[0, 4]"), "time_signature numerator 0 is outside 1..=32");
        refused(&single_with("[4, 4]", "[33, 4]"), "time_signature numerator 33 is outside");
        refused(&single_with("[4, 4]", "[4, 3]"), "time_signature denominator 3 is not one of 1, 2, 4, 8, 16, 32");
        refused(&single_with("[4, 4]", "[4, 64]"), "time_signature denominator 64 is not one of");
        refused(&single_with(r#""tracks": ["#, r#""tracks": {"a":["#).replace("} ] } ]", "} ] } ]}"), "tracks must be an array");
        refused(
            &single_with(
                r#""tracks": [ { "name": "lead", "channel": 0, "program": 0,
                      "notes": [ { "pitch": 60, "velocity": 100, "onset": 0, "duration": 480 } ] } ]"#,
                r#""tracks": []"#,
            ),
            "tracks must hold 1..=64 tracks, not 0",
        );
        refused(&single_with(r#""tracks": [ {"#, r#""tracks": [ 5, {"#), "track 0 is not an object");
        refused(&single_with(r#""name": "lead","#, r#""name": "lead", "mute": true,"#), "track 0: unknown key \"mute\"");
        refused(&single_with(r#""channel": 0,"#, ""), "track 0: missing key \"channel\"");
        refused(&single_with(r#""name": "lead","#, r#""name": 7,"#), "track 0 name must be a string");
        refused(
            &single_with(r#""name": "lead","#, &format!(r#""name": "{}","#, "x".repeat(65))),
            "track 0 name is 65 bytes; at most 64",
        );
        refused(&single_with(r#""channel": 0,"#, r#""channel": 16,"#), "track 0 channel 16 is outside 0..=15");
        refused(&single_with(r#""channel": 0,"#, r#""channel": -1,"#), "track 0 channel -1 is outside");
        refused(&single_with(r#""program": 0,"#, r#""program": 128,"#), "track 0 program 128 is outside 0..=127");
        refused(
            &single_with(
                r#""notes": [ { "pitch": 60, "velocity": 100, "onset": 0, "duration": 480 } ]"#,
                r#""notes": { "pitch": 60 }"#,
            ),
            "track 0 notes must be an array",
        );
        refused(&single_with(r#""notes": [ {"#, r#""notes": [ "c4", {"#), "track 0 note 0 is not an object");
        refused(&single_with(r#""pitch": 60,"#, r#""pitch": 60, "tie": 1,"#), "track 0 note 0: unknown key \"tie\"");
        refused(&single_with(r#""velocity": 100,"#, ""), "track 0 note 0: missing key \"velocity\"");
        refused(&single_with(r#""pitch": 60,"#, r#""pitch": 128,"#), "track 0 note 0 pitch 128 is outside 0..=127");
        refused(&single_with(r#""velocity": 100,"#, r#""velocity": 0,"#), "track 0 note 0 velocity 0 is outside 1..=127");
        refused(&single_with(r#""velocity": 100,"#, r#""velocity": null,"#), "track 0 note 0 velocity must be an integer");
        refused(&single_with(r#""onset": 0,"#, r#""onset": 268435456,"#), "track 0 note 0 onset 268435456 is outside 0..=268435455");
        refused(&single_with(r#""onset": 0,"#, r#""onset": -1,"#), "track 0 note 0 onset -1 is outside");
        refused(&single_with(r#""duration": 480"#, r#""duration": 0"#), "track 0 note 0 duration 0 is outside 1..=268435455");
        refused(&single_with(r#""duration": 480"#, r#""duration": 268435456"#), "track 0 note 0 duration 268435456 is outside");
        refused(
            &single_with(r#""onset": 0,"#, r#""onset": 2,"#).replacen(r#""duration": 480"#, r#""duration": 268435455"#, 1),
            "ends at tick 268435457, past 268435456",
        );
        // the boundary itself is admitted: onset + duration == 2^28
        let at_the_edge =
            single_with(r#""onset": 0,"#, r#""onset": 1,"#).replacen(r#""duration": 480"#, r#""duration": 268435455"#, 1);
        assert!(MusicGrammar.canonicalize(at_the_edge.as_bytes()).is_ok());
    }

    #[test]
    fn refuses_more_than_the_bounds_of_tracks_and_notes() {
        let track = r#"{"name":"t","channel":0,"program":0,"notes":[]}"#;
        let tracks = |n: usize| {
            format!(r#"{{"v":1,"ppq":96,"tempo_us_per_quarter":1,"time_signature":[1,1],"tracks":[{}]}}"#, vec![track; n].join(","))
        };
        assert!(MusicGrammar.canonicalize(tracks(64).as_bytes()).is_ok());
        refused(&tracks(65), "tracks must hold 1..=64 tracks, not 65");

        let note = r#"{"pitch":1,"velocity":1,"onset":0,"duration":1}"#;
        let notes = |n: usize| {
            format!(
                r#"{{"v":1,"ppq":96,"tempo_us_per_quarter":1,"time_signature":[1,1],"tracks":[{{"name":"","channel":0,"program":0,"notes":[{}]}},{{"name":"","channel":1,"program":0,"notes":[{}]}}]}}"#,
                vec![note; n / 2].join(","),
                vec![note; n - n / 2].join(",")
            )
        };
        assert!(MusicGrammar.canonicalize(notes(65_536).as_bytes()).is_ok());
        refused(&notes(65_537), "more than 65536 notes in all");
    }

    #[test]
    fn the_transformer_refuses_input_that_is_not_canonical() {
        for bad in [SINGLE.as_bytes(), b"{", br#"{"v":2}"#, b""] {
            match MusicSmfTransformer.run(bad) {
                Err(DeriveError::Transformer(msg)) => assert!(msg.contains("not canonical music/v1"), "{msg}"),
                other => panic!("expected a transformer refusal, got {other:?}"),
            }
        }
        // one byte of whitespace past the canonical form is enough
        let mut padded = canonical(SINGLE);
        padded.push(b'\n');
        assert!(matches!(MusicSmfTransformer.run(&padded), Err(DeriveError::Transformer(_))));
        assert!(check_artifact_size(ARTIFACT_MAX_BYTES).is_ok());
        assert!(matches!(check_artifact_size(ARTIFACT_MAX_BYTES + 1), Err(DeriveError::Transformer(_))));
    }

    // ---- (3) determinism -------------------------------------------------------------------

    #[test]
    fn the_same_dsl_twice_is_the_same_bytes() {
        assert_eq!(artifact(CHORDS), artifact(CHORDS));
        assert_eq!(artifact(SINGLE), artifact(SINGLE));
    }

    #[test]
    fn key_order_and_whitespace_change_nothing() {
        let reordered = r#"{"tracks":[{"notes":[{"duration":480,"onset":0,"velocity":100,"pitch":60}],"program":0,"channel":0,"name":"lead"}],"time_signature":[4,4],"tempo_us_per_quarter":500000,"ppq":480,"v":1}"#;
        assert_eq!(canonical(reordered), canonical(SINGLE));
        assert_eq!(artifact(reordered), artifact(SINGLE));
    }

    #[test]
    fn note_order_in_the_answer_does_not_reach_the_artifact() {
        // CHORDS lists its notes out of order; the same notes sorted by onset and pitch:
        let sorted = r#"{"v":1,"ppq":96,"tempo_us_per_quarter":600000,"time_signature":[4,4],"tracks":[
          {"name":"piano","channel":0,"program":0,"notes":[
            {"pitch":60,"velocity":96,"onset":0,"duration":384},
            {"pitch":64,"velocity":96,"onset":0,"duration":384},
            {"pitch":67,"velocity":96,"onset":0,"duration":384},
            {"pitch":57,"velocity":96,"onset":384,"duration":384},
            {"pitch":60,"velocity":96,"onset":384,"duration":384},
            {"pitch":64,"velocity":96,"onset":384,"duration":384}]},
          {"name":"bass","channel":1,"program":33,"notes":[
            {"pitch":36,"velocity":80,"onset":0,"duration":192},
            {"pitch":36,"velocity":80,"onset":192,"duration":192},
            {"pitch":33,"velocity":80,"onset":384,"duration":192}]}]}"#;
        // the canonical DSL keeps the answer's order (Decision 2: nothing semantic) ...
        assert_ne!(canonical(sorted), canonical(CHORDS));
        // ... and the writer's sort makes the artifact the same bytes
        assert_eq!(artifact(sorted), artifact(CHORDS));
        // a fully reversed note list, likewise
        let mut reversed = song(CHORDS);
        for t in &mut reversed.tracks {
            t.notes.reverse();
        }
        assert_eq!(write_smf(&reversed).unwrap(), artifact(CHORDS));
    }

    // ---- (4) a structural walk of the bytes -----------------------------------------------

    struct Chunk<'a> {
        tag: &'a [u8],
        body: &'a [u8],
    }

    fn chunks(bytes: &[u8]) -> Vec<Chunk<'_>> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let len = u32::from_be_bytes(bytes[i + 4..i + 8].try_into().unwrap()) as usize;
            assert!(i + 8 + len <= bytes.len(), "chunk at {i} claims {len} bytes past the end");
            out.push(Chunk { tag: &bytes[i..i + 4], body: &bytes[i + 8..i + 8 + len] });
            i += 8 + len;
        }
        assert_eq!(i, bytes.len());
        out
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Ev {
        Meta { delta: u32, ty: u8, data: Vec<u8> },
        Chan { delta: u32, status: u8, data: Vec<u8> },
    }

    fn decode_vlq(b: &[u8], i: &mut usize) -> u32 {
        let start = *i;
        let mut v = 0u32;
        loop {
            let byte = b[*i];
            *i += 1;
            v = (v << 7) | (byte & 0x7F) as u32;
            if byte & 0x80 == 0 {
                break;
            }
        }
        assert!(*i - start <= 4, "a variable-length quantity longer than four bytes");
        let mut again = Vec::new();
        encode_vlq(v, &mut again);
        assert_eq!(again, &b[start..*i], "the VLQ does not round-trip");
        v
    }

    fn parse_track(body: &[u8]) -> Vec<Ev> {
        let mut i = 0;
        let mut events = Vec::new();
        while i < body.len() {
            let delta = decode_vlq(body, &mut i);
            let status = body[i];
            i += 1;
            assert!(status & 0x80 != 0, "running status at offset {i}: every event carries its status byte");
            if status == 0xFF {
                let ty = body[i];
                i += 1;
                let len = decode_vlq(body, &mut i) as usize;
                events.push(Ev::Meta { delta, ty, data: body[i..i + len].to_vec() });
                i += len;
            } else {
                let n = match status & 0xF0 {
                    0xC0 | 0xD0 => 1,
                    0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => 2,
                    other => panic!("unexpected status {other:#04x}"),
                };
                events.push(Ev::Chan { delta, status, data: body[i..i + n].to_vec() });
                i += n;
            }
        }
        assert_eq!(i, body.len());
        assert!(matches!(events.last(), Some(Ev::Meta { delta: 0, ty: META_END_OF_TRACK, .. })), "no end of track");
        events
    }

    fn walk(answer: &str) -> (Song, Vec<Vec<Ev>>) {
        let song = song(answer);
        let bytes = artifact(answer);
        let chunks = chunks(&bytes);
        assert_eq!(chunks.len(), 2 + song.tracks.len());
        let head = &chunks[0];
        assert_eq!(head.tag, b"MThd");
        assert_eq!(head.body.len(), 6);
        assert_eq!(&head.body[0..2], &[0, 1], "format 1");
        assert_eq!(u16::from_be_bytes([head.body[2], head.body[3]]) as usize, 1 + song.tracks.len(), "ntrks");
        assert_eq!(u16::from_be_bytes([head.body[4], head.body[5]]), song.ppq, "division");
        let mut tracks = Vec::new();
        for c in &chunks[1..] {
            assert_eq!(c.tag, b"MTrk");
            tracks.push(parse_track(c.body));
        }
        (song, tracks)
    }

    #[test]
    fn the_bytes_are_the_structure_the_writer_states() {
        let (song, tracks) = walk(CHORDS);
        let t = song.tempo_us_per_quarter;
        assert_eq!(
            tracks[0],
            vec![
                Ev::Meta { delta: 0, ty: META_TRACK_NAME, data: b"tempo".to_vec() },
                Ev::Meta { delta: 0, ty: META_TEMPO, data: vec![(t >> 16) as u8, (t >> 8) as u8, t as u8] },
                Ev::Meta { delta: 0, ty: META_TIME_SIGNATURE, data: vec![4, 2, 24, 8] },
                Ev::Meta { delta: 0, ty: META_END_OF_TRACK, data: vec![] },
            ]
        );
        let mut same_tick_off_then_on = 0;
        for (track, events) in song.tracks.iter().zip(&tracks[1..]) {
            assert_eq!(events[0], Ev::Meta { delta: 0, ty: META_TRACK_NAME, data: track.name.as_bytes().to_vec() });
            assert_eq!(events[1], Ev::Chan { delta: 0, status: STATUS_PROGRAM_CHANGE | track.channel, data: vec![track.program] });
            let expected = track_events(track);
            let notes = &events[2..events.len() - 1];
            assert_eq!(notes.len(), expected.len());
            let mut tick = 0u32;
            let mut previous: Option<Event> = None;
            for (ev, (etick, eclass, epitch, evelocity)) in notes.iter().zip(expected) {
                let Ev::Chan { delta, status, data } = ev else { panic!("a meta event among the notes: {ev:?}") };
                tick += delta;
                assert_eq!(tick, etick);
                let expected_status = if eclass == EVENT_NOTE_OFF { STATUS_NOTE_OFF } else { STATUS_NOTE_ON } | track.channel;
                assert_eq!(*status, expected_status);
                assert_eq!(data, &vec![epitch, evelocity]);
                if eclass == EVENT_NOTE_OFF {
                    assert_eq!(evelocity, NOTE_OFF_VELOCITY);
                }
                let key: Event = (tick, eclass, epitch, evelocity);
                if let Some(p) = previous {
                    assert!(p <= key, "events out of canonical order: {p:?} then {key:?}");
                    if p.0 == key.0 && p.1 == EVENT_NOTE_ON {
                        assert_eq!(key.1, EVENT_NOTE_ON, "a note-off after a note-on at tick {tick}");
                    }
                    if p.0 == key.0 && p.1 == EVENT_NOTE_OFF && key.1 == EVENT_NOTE_ON {
                        same_tick_off_then_on += 1;
                    }
                }
                previous = Some(key);
            }
        }
        assert!(same_tick_off_then_on > 0, "the sample was meant to have a bar line with an off and an on at one tick");
    }

    // ---- (5) the VLQ table from the MIDI specification ------------------------------------

    #[test]
    fn vlq_matches_the_specification_table() {
        for (value, bytes) in [
            (0u32, &[0x00u8][..]),
            (0x40, &[0x40]),
            (0x7F, &[0x7F]),
            (0x80, &[0x81, 0x00]),
            (0x2000, &[0xC0, 0x00]),
            (0x3FFF, &[0xFF, 0x7F]),
            (0x4000, &[0x81, 0x80, 0x00]),
            (0x10_0000, &[0xC0, 0x80, 0x00]),
            (0x1F_FFFF, &[0xFF, 0xFF, 0x7F]),
            (0x20_0000, &[0x81, 0x80, 0x80, 0x00]),
            (0x0800_0000, &[0xC0, 0x80, 0x80, 0x00]),
            (0x0FFF_FFFF, &[0xFF, 0xFF, 0xFF, 0x7F]),
        ] {
            let mut out = Vec::new();
            encode_vlq(value, &mut out);
            assert_eq!(out, bytes, "{value:#x}");
        }
        let mut out = Vec::new();
        assert!(delta_time(&mut out, VLQ_MAX).is_ok());
        assert!(delta_time(&mut out, VLQ_MAX + 1).is_err());
    }

    // ---- (6) the fixture corpus ------------------------------------------------------------

    fn corpus_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus").join("music")
    }

    /// Every `*.json` sample in the corpus, by file name, `golden.json` excluded.
    fn corpus() -> BTreeMap<String, Vec<u8>> {
        let mut files = BTreeMap::new();
        for entry in std::fs::read_dir(corpus_dir()).expect("corpus/music exists") {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            if name.ends_with(".json") && name != "golden.json" {
                files.insert(name, std::fs::read(&path).unwrap());
            }
        }
        assert!(files.len() >= 4, "the corpus holds {} samples; at least four are expected", files.len());
        files
    }

    #[test]
    fn corpus_derives_to_the_golden_values() {
        let golden: serde_json::Value = serde_json::from_slice(&std::fs::read(corpus_dir().join("golden.json")).unwrap()).unwrap();
        let golden = golden.as_object().unwrap();
        let files = corpus();
        let grammar_id = grammar_id_v1(GRAMMAR_NAME);
        for (name, answer) in &files {
            let d = derive_with(&MusicGrammar, &MusicSmfTransformer, &binding(), answer).unwrap_or_else(|e| panic!("{name}: {e}"));
            let g = golden.get(name).unwrap_or_else(|| panic!("{name} has no entry in golden.json; pin it"));
            assert_eq!(g["dsl_hash"].as_str().unwrap(), d.dsl_hash.to_string(), "{name} dsl_hash");
            assert_eq!(g["artifact_hash"].as_str().unwrap(), d.artifact_hash.to_string(), "{name} artifact_hash");
            assert_eq!(g["artifact_bytes"].as_u64().unwrap(), d.object.artifact_bytes, "{name} artifact_bytes");
            assert_eq!(d.object.artifact_bytes as usize, d.artifact.bytes.len());
            // the ids recomputed directly, the way a consumer does (Decision 5)
            assert_eq!(dsl_hash_v1(&grammar_id, &d.canonical_dsl), d.dsl_hash);
            assert_eq!(artifact_hash_v1(&d.artifact.bytes), d.artifact_hash);
            assert_eq!(d.grammar_id, grammar_id);
            assert_eq!(d.kind, kind::MUSIC);
            // the registry route names the same derivation, and verification agrees with it
            let named = derive_named(TRANSFORMER_NAME, &binding(), answer).unwrap();
            assert_eq!(named.object, d.object);
            assert!(crate::verify(&d.object, answer).unwrap().all_match(), "{name}");
            assert!(crate::verify_artifact_bytes(&d.object, &d.artifact.bytes));
            // a second run is the same bytes
            assert_eq!(MusicSmfTransformer.run(&d.canonical_dsl).unwrap().bytes, d.artifact.bytes);
            // and the bytes walk as a well-formed SMF
            walk(std::str::from_utf8(answer).unwrap());
        }
        for name in golden.keys() {
            assert!(files.contains_key(name), "golden.json names {name}, which is not in the corpus");
        }
    }

    /// Re-pin: `cargo test -p misaka-palw-derive print_golden -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn print_golden() {
        let mut out = serde_json::Map::new();
        for (name, answer) in &corpus() {
            let d = derive_with(&MusicGrammar, &MusicSmfTransformer, &binding(), answer).unwrap();
            let mut entry = serde_json::Map::new();
            entry.insert("dsl_hash".into(), d.dsl_hash.to_string().into());
            entry.insert("artifact_hash".into(), d.artifact_hash.to_string().into());
            entry.insert("artifact_bytes".into(), d.object.artifact_bytes.into());
            out.insert(name.clone(), entry.into());
        }
        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(out)).unwrap());
    }

    // ---- (7) the discipline, scanned -------------------------------------------------------

    #[test]
    fn no_floating_point_token_in_this_file() {
        let source = include_str!("music.rs");
        for width in [32u32, 64] {
            let token = format!("f{width}");
            assert!(!source.contains(&token), "music.rs mentions {token}");
        }
    }
}
