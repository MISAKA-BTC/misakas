//! **RFC-0003 §I.1.6: RFC-0001's D11 sampler is R's domain 0, byte for byte.**
//!
//! Three independent checks. None of them compiles `kaspa-consensus-core`; all of them bind to it.
//!
//! 1. **The source.** `consensus/core/src/palw_decode_select_v2.rs` — identical on `rcore/fp-sampler`
//!    (6cccbab3e, RFC-0001's train) and on this line — is read as text. Its domain string, index
//!    width, Gumbel table and the table's pin are parsed; the table is re-hashed to the pin; and the
//!    bodies of `gumbel_index_v1`, `gumbel_q24_v1` and `decode_lane_key_v2` must be exactly the texts
//!    below. Any edit to D11 fails here first.
//! 2. **The function.** A transcription of `gumbel_index_v1`'s body is compared with
//!    `rand_word_v1(TEXT_GUMBEL_V1, …)` — the generic R, through the domain table and the
//!    big-endian word reader — over a sweep of seeds, positions and lanes, edges included.
//! 3. **RFC-0001's own golden vectors.** `consensus-vectors/fp-v4/processor_order.json` and
//!    `v4_noop_equals_v3.json` of `rcore/fp-sampler` at 6cccbab3e, copied verbatim into
//!    `tests/data/rfc0001-fp-v4/` (delete the copies and point here at `consensus-vectors/fp-v4/`
//!    once that train lands). Every selection in them — sampled and greedy, masked and not — is
//!    recomputed with D11's lane key over R's domain 0 and the parsed table, and must pick the lane
//!    the vector says.

use misaka_palw_gen::rand::{GUMBEL_Q24_V1_DIGEST_HEX, RAND_DOMAINS_V1, RandLayoutV1, RandStepRuleV1, TEXT_GUMBEL_V1, rand_word_v1};
use serde::Deserialize;

const D11_SOURCE: &str = include_str!("../../consensus/core/src/palw_decode_select_v2.rs");

const D11_GUMBEL_INDEX_V1: &str = "pub fn gumbel_index_v1(seed: &[u8; 32], position: u32, lane: usize) -> usize {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_DECODE_SELECT_V2_GUMBEL_DOMAIN).to_state();
    state.update(seed);
    state.update(&position.to_le_bytes());
    state.update(&(lane as u64).to_le_bytes());
    let digest = state.finalize();
    let bytes = digest.as_bytes();
    let head = u16::from_be_bytes([bytes[0], bytes[1]]);
    (head >> (16 - PALW_GUMBEL_TABLE_INDEX_BITS)) as usize
}
";

const D11_GUMBEL_Q24_V1: &str = "pub fn gumbel_q24_v1(seed: &[u8; 32], position: u32, lane: usize) -> i32 {
    PALW_GUMBEL_Q24_V1[gumbel_index_v1(seed, position, lane)]
}
";

const D11_DECODE_LANE_KEY_V2: &str =
    "pub fn decode_lane_key_v2(value: i32, seed: &[u8; 32], position: u32, lane: usize, temperature_q: u32) -> i64 {
    let base = (value as i64) * PALW_DECODE_T_ONE;
    if temperature_q == PALW_DECODE_TEMPERATURE_GREEDY {
        return base;
    }
    base + (((temperature_q as i64) * (gumbel_q24_v1(seed, position, lane) as i64)) >> K)
}
";

/// D11's fixed point, `palw_base0::K` (`PALW_DECODE_T_ONE = 1 << K`).
const K: u32 = 24;

fn between<'a>(s: &'a str, start: &str, end: &str) -> &'a str {
    let i = s.find(start).unwrap_or_else(|| panic!("the D11 source no longer contains {start:?}")) + start.len();
    let j = s[i..].find(end).unwrap_or_else(|| panic!("no {end:?} after {start:?}")) + i;
    &s[i..j]
}

struct D11 {
    domain: Vec<u8>,
    index_bits: u32,
    table: Vec<i32>,
    digest_hex: String,
}

fn parse_d11() -> D11 {
    let domain = between(D11_SOURCE, "pub const PALW_DECODE_SELECT_V2_GUMBEL_DOMAIN: &[u8] = b\"", "\";").as_bytes().to_vec();
    let index_bits = between(D11_SOURCE, "pub const PALW_GUMBEL_TABLE_INDEX_BITS: u32 = ", ";").trim().parse().expect("the index width");
    let digest_hex = between(D11_SOURCE, "pub const PALW_GUMBEL_Q24_V1_DIGEST_HEX: &str =", ";").trim().trim_matches('"').to_string();
    let table = between(D11_SOURCE, "pub static PALW_GUMBEL_Q24_V1: [i32; PALW_GUMBEL_TABLE_LEN] = [", "];")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<i32>().expect("a table entry"))
        .collect();
    D11 { domain, index_bits, table, digest_hex }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn seed_from_hex(s: &str) -> [u8; 32] {
    assert_eq!(s.len(), 64, "a seed is 32 bytes");
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).expect("hex");
    }
    out
}

/// `gumbel_index_v1`'s body, transcribed (check 1 pins the original to exactly this text).
fn d11_gumbel_index_transcribed(domain: &[u8], seed: &[u8; 32], position: u32, lane: usize) -> usize {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    state.update(seed);
    state.update(&position.to_le_bytes());
    state.update(&(lane as u64).to_le_bytes());
    let digest = state.finalize();
    let bytes = digest.as_bytes();
    let head = u16::from_be_bytes([bytes[0], bytes[1]]);
    (head >> (16 - 13)) as usize
}

#[test]
fn check_1_the_source_is_what_domain_0_claims() {
    let d11 = parse_d11();
    let d0 = &RAND_DOMAINS_V1[TEXT_GUMBEL_V1 as usize];
    assert_eq!(d0.key, d11.domain.as_slice(), "domain 0's key is D11's domain string");
    assert_eq!(d0.word_bits, d11.index_bits, "domain 0's word is D11's index width");
    assert_eq!((d0.layout, d0.step, d0.words_per_digest()), (RandLayoutV1::TextGumbel, RandStepRuleV1::None, 1));
    assert_eq!(d11.table.len(), 1 << d11.index_bits, "the Gumbel table has one entry per index");
    let mut state = blake2b_simd::Params::new().hash_length(64).key(&d11.domain).to_state();
    for e in &d11.table {
        state.update(&e.to_le_bytes());
    }
    assert_eq!(hex(state.finalize().as_bytes()), d11.digest_hex, "the parsed table is the pinned one");
    assert_eq!(d11.digest_hex, GUMBEL_Q24_V1_DIGEST_HEX, "the rand set names D11's table by its pin");
    for (name, text) in
        [("gumbel_index_v1", D11_GUMBEL_INDEX_V1), ("gumbel_q24_v1", D11_GUMBEL_Q24_V1), ("decode_lane_key_v2", D11_DECODE_LANE_KEY_V2)]
    {
        assert!(D11_SOURCE.contains(text), "{name} is no longer the function this proof transcribes");
    }
    assert!(D11_SOURCE.contains("pub const PALW_DECODE_T_ONE: i64 = 1i64 << K;"));
    assert!(D11_SOURCE.contains("pub const PALW_DECODE_TEMPERATURE_GREEDY: u32 = 0;"));
}

/// A 64-bit LCG, so the sweep depends on no RNG crate's stream.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 ^ (self.0 >> 29)
    }
}

#[test]
fn check_2_generic_r_is_the_transcribed_index_on_a_sweep() {
    let d11 = parse_d11();
    let mut rng = Lcg(0x5eed_d11d);
    let mut cases: Vec<([u8; 32], u32, u64)> = Vec::new();
    for position in [0u32, 1, 2, 255, 256, 65_535, u32::MAX - 1, u32::MAX] {
        for lane in [0u64, 1, 2, 7, 8, 151_935, 151_936, u32::MAX as u64, (1u64 << 40) + 3] {
            cases.push(([0u8; 32], position, lane));
            cases.push(([0xffu8; 32], position, lane));
        }
    }
    for _ in 0..20_000 {
        let mut seed = [0u8; 32];
        for chunk in seed.chunks_mut(8) {
            chunk.copy_from_slice(&rng.next().to_le_bytes());
        }
        cases.push((seed, (rng.next() % (1 << 20)) as u32, rng.next() % 200_000));
    }
    for (seed, position, lane) in cases {
        let want = d11_gumbel_index_transcribed(&d11.domain, &seed, position, lane as usize) as u32;
        let got = rand_word_v1(TEXT_GUMBEL_V1, &seed, 0, position, lane).expect("domain 0 draws");
        assert_eq!(got, want, "seed {} position {position} lane {lane}", hex(&seed));
    }
}

#[derive(Deserialize)]
struct ProcessorFile {
    cases: Vec<ProcessorCase>,
}

#[derive(Deserialize)]
struct ProcessorCase {
    name: String,
    seed: String,
    temperature_q: u32,
    generated_before: Vec<u32>,
    admitted: Option<Vec<usize>>,
    processed: Vec<Option<i32>>,
    expected_lane: Option<usize>,
}

#[derive(Deserialize)]
struct NoopFile {
    cases: Vec<NoopCase>,
}

#[derive(Deserialize)]
struct NoopCase {
    seed: String,
    temperature_q: u32,
    generated_before: Vec<u32>,
    row: Vec<i32>,
    expected_lane: usize,
}

/// D11's lane key (`decode_lane_key_v2`, pinned by check 1) with its Gumbel lookup routed through R's
/// domain 0 and the parsed table.
fn lane_key(table: &[i32], value: i32, seed: &[u8; 32], position: u32, lane: usize, temperature_q: u32) -> i64 {
    let base = (value as i64) * (1i64 << K);
    if temperature_q == 0 {
        return base;
    }
    let word = rand_word_v1(TEXT_GUMBEL_V1, seed, 0, position, lane as u64).expect("domain 0 draws");
    base + (((temperature_q as i64) * (table[word as usize] as i64)) >> K)
}

/// The argmax over the lanes that carry a value and are admitted, ties to the LOWEST index
/// (`decode_lane_beats_v2`).
fn select(table: &[i32], values: &[Option<i32>], admitted: &dyn Fn(usize) -> bool, seed: &[u8; 32], position: u32, t: u32) -> Option<usize> {
    let mut best: Option<(usize, i64)> = None;
    for (lane, v) in values.iter().enumerate() {
        let Some(v) = v else { continue };
        if !admitted(lane) {
            continue;
        }
        let key = lane_key(table, *v, seed, position, lane, t);
        if best.is_none_or(|(_, bk)| key > bk) {
            best = Some((lane, key));
        }
    }
    best.map(|(lane, _)| lane)
}

#[test]
fn check_3_rfc0001_golden_selections_replay_through_domain_0() {
    let d11 = parse_d11();
    let file: ProcessorFile = serde_json::from_str(include_str!("data/rfc0001-fp-v4/processor_order.json")).expect("processor_order.json");
    let mut sampled = 0;
    for c in &file.cases {
        let seed = seed_from_hex(&c.seed);
        let admitted = |lane: usize| c.admitted.as_ref().is_none_or(|set| set.contains(&lane));
        let got = select(&d11.table, &c.processed, &admitted, &seed, c.generated_before.len() as u32, c.temperature_q);
        assert_eq!(got, c.expected_lane, "processor_order {}: T {}", c.name, c.temperature_q);
        sampled += (c.temperature_q != 0) as usize;
    }
    let noop: NoopFile = serde_json::from_str(include_str!("data/rfc0001-fp-v4/v4_noop_equals_v3.json")).expect("v4_noop_equals_v3.json");
    for (i, c) in noop.cases.iter().enumerate() {
        let seed = seed_from_hex(&c.seed);
        let values: Vec<Option<i32>> = c.row.iter().map(|v| Some(*v)).collect();
        let got = select(&d11.table, &values, &|_| true, &seed, c.generated_before.len() as u32, c.temperature_q);
        assert_eq!(got, Some(c.expected_lane), "v4_noop_equals_v3 case {i}: T {}", c.temperature_q);
        sampled += (c.temperature_q != 0) as usize;
    }
    assert!(file.cases.len() + noop.cases.len() >= 400 && sampled >= 100, "the vectors still exercise sampling");
}
