//! **RFC-0001 §A.5 — the consensus golden vectors, read** (`consensus-vectors/fp-v4/`).
//!
//! The vectors are the executable form of the frozen spec: written by
//! `scripts/palw-fp-v4-vectors.py`, a second implementation of §A.3 that shares nothing with the
//! Rust one but the Gumbel table's generator. The sampler (this crate), the worker
//! (`misaka-palw-base0`) and the panel (`kaspad`) each `include_str!` the SAME files and hand
//! them to the checkers below with their OWN entry point — the function their engine or replay
//! actually calls — so "the worker and the seat run the spec" is a test, not a belief.
//!
//! Pure: no file IO here (a caller includes the bytes), and every checker answers the number of
//! cases it compared or the first disagreement, by case.

use crate::palw_decode_pipeline_v4::{
    DecodeConfigV4, PalwDecodeConfigV4Error, PalwFpDecodeStopReasonV1, PalwFpDecodeStopV1, decode_first_stop_v4,
    decode_frequency_presence_v4, decode_lane_value_v4, decode_repeat_v4, decode_stop_match_v4, decode_window_count_v4,
};
use crate::palw_decode_select_v2::PalwDecodeSamplingV2;
use serde::Deserialize;

/// The files, by name, as the RFC lists them (`job_v4_encoding.json` is the wire's, read by
/// `palw_freeprompt_v3`'s tests).
pub const FP_V4_VECTOR_FILES: [&str; 7] = [
    "repeat_penalty.json",
    "frequency_penalty.json",
    "presence_penalty.json",
    "logit_bias.json",
    "stop_sequences.json",
    "processor_order.json",
    "v4_noop_equals_v3.json",
];

/// The name a canonical-form refusal carries in the vectors: the Rust variant's name.
pub fn fp_v4_error_name(error: &PalwDecodeConfigV4Error) -> &'static str {
    use PalwDecodeConfigV4Error::*;
    match error {
        RepeatPenaltyOutOfRange { .. } => "RepeatPenaltyOutOfRange",
        FrequencyPenaltyOutOfRange { .. } => "FrequencyPenaltyOutOfRange",
        PresencePenaltyOutOfRange { .. } => "PresencePenaltyOutOfRange",
        PenaltyWindowOutOfRange { .. } => "PenaltyWindowOutOfRange",
        PenaltyWindowWithoutPenalty { .. } => "PenaltyWindowWithoutPenalty",
        TooManyBiasEntries { .. } => "TooManyBiasEntries",
        BiasNotAscending { .. } => "BiasNotAscending",
        BiasOutOfRange { .. } => "BiasOutOfRange",
        ZeroBias { .. } => "ZeroBias",
        TooManyStopSequences { .. } => "TooManyStopSequences",
        EmptyStopSequence { .. } => "EmptyStopSequence",
        StopSequenceTooLong { .. } => "StopSequenceTooLong",
        StopSequencesNotAscending { .. } => "StopSequencesNotAscending",
    }
}

/// The name a stop carries in the vectors.
pub fn fp_v4_stop_name(reason: PalwFpDecodeStopReasonV1) -> String {
    match reason {
        PalwFpDecodeStopReasonV1::Budget => "Budget".to_string(),
        PalwFpDecodeStopReasonV1::StopSequence { index } => format!("StopSequence:{index}"),
        PalwFpDecodeStopReasonV1::NoAdmissibleLane => "NoAdmissibleLane".to_string(),
    }
}

fn sampling_of(seed_hex: &str, temperature_q: u32) -> Result<PalwDecodeSamplingV2, String> {
    let mut seed = [0u8; 32];
    faster_hex::hex_decode(seed_hex.as_bytes(), &mut seed).map_err(|e| format!("seed {seed_hex}: {e}"))?;
    Ok(PalwDecodeSamplingV2 { seed, temperature_q })
}

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| format!("the vector file does not parse: {e}"))
}

#[derive(Deserialize)]
struct RepeatFile {
    cases: Vec<RepeatCase>,
}
#[derive(Deserialize)]
struct RepeatCase {
    value: i32,
    count: u32,
    repeat_penalty_q: u32,
    expected: i64,
}

/// `repeat_penalty.json` against [`decode_repeat_v4`].
pub fn fp_v4_check_repeat_vectors(json: &str) -> Result<usize, String> {
    let file: RepeatFile = parse(json)?;
    for (i, c) in file.cases.iter().enumerate() {
        let got = decode_repeat_v4(c.value, c.count, c.repeat_penalty_q);
        if got != c.expected {
            return Err(format!(
                "repeat case {i}: v {} c {} p {} → {got}, the vector says {}",
                c.value, c.count, c.repeat_penalty_q, c.expected
            ));
        }
    }
    Ok(file.cases.len())
}

#[derive(Deserialize)]
struct PenaltyFile {
    cases: Vec<PenaltyCase>,
}
#[derive(Deserialize)]
struct PenaltyCase {
    repeated: i64,
    count: u32,
    #[serde(default)]
    frequency_penalty_q: i32,
    #[serde(default)]
    presence_penalty_q: i32,
    expected: i64,
}

/// `frequency_penalty.json` and `presence_penalty.json` against [`decode_frequency_presence_v4`].
pub fn fp_v4_check_penalty_vectors(json: &str) -> Result<usize, String> {
    let file: PenaltyFile = parse(json)?;
    for (i, c) in file.cases.iter().enumerate() {
        let got = decode_frequency_presence_v4(c.repeated, c.count, c.frequency_penalty_q, c.presence_penalty_q);
        if got != c.expected {
            return Err(format!("penalty case {i}: → {got}, the vector says {}", c.expected));
        }
    }
    Ok(file.cases.len())
}

#[derive(Deserialize)]
struct BiasFile {
    canonical: Vec<BiasCanonical>,
    lanes: Vec<BiasLane>,
}
#[derive(Deserialize)]
struct BiasCanonical {
    logit_bias: Vec<(u32, i32)>,
    error: Option<String>,
}
#[derive(Deserialize)]
struct BiasLane {
    logit_bias: Vec<(u32, i32)>,
    lane: u32,
    value: i32,
    expected: Option<i32>,
}

/// `logit_bias.json`: the canonical form's refusals by name, and steps 3–5 per lane.
pub fn fp_v4_check_logit_bias_vectors(json: &str) -> Result<usize, String> {
    let file: BiasFile = parse(json)?;
    for (i, c) in file.canonical.iter().enumerate() {
        let config = DecodeConfigV4 { logit_bias: c.logit_bias.clone(), ..DecodeConfigV4::NOOP };
        let got = config.validate_canonical().err().map(|e| fp_v4_error_name(&e).to_string());
        if got != c.error {
            return Err(format!("logit_bias canonical case {i}: {got:?}, the vector says {:?}", c.error));
        }
    }
    for (i, c) in file.lanes.iter().enumerate() {
        let config = DecodeConfigV4 { logit_bias: c.logit_bias.clone(), ..DecodeConfigV4::NOOP };
        let got = decode_lane_value_v4(&config, c.value, c.lane, 0);
        if got != c.expected {
            return Err(format!("logit_bias lane case {i}: lane {} → {got:?}, the vector says {:?}", c.lane, c.expected));
        }
    }
    Ok(file.canonical.len() + file.lanes.len())
}

#[derive(Deserialize)]
struct StopFile {
    canonical: Vec<StopCanonical>,
    matches: Vec<StopMatch>,
    first_stop: Vec<StopFirst>,
}
#[derive(Deserialize)]
struct StopCanonical {
    stop_sequences: Vec<Vec<u32>>,
    error: Option<String>,
}
#[derive(Deserialize)]
struct StopMatch {
    stop_sequences: Vec<Vec<u32>>,
    generated: Vec<u32>,
    expected: Option<usize>,
}
#[derive(Deserialize)]
struct StopFirst {
    stop_sequences: Vec<Vec<u32>>,
    answer: Vec<u32>,
    expected: Option<(usize, usize)>,
}

/// `stop_sequences.json`: the canonical form, the tail matcher, and a finished answer's first stop.
pub fn fp_v4_check_stop_vectors(json: &str) -> Result<usize, String> {
    let file: StopFile = parse(json)?;
    for (i, c) in file.canonical.iter().enumerate() {
        let config = DecodeConfigV4 { stop_sequences: c.stop_sequences.clone(), ..DecodeConfigV4::NOOP };
        let got = config.validate_canonical().err().map(|e| fp_v4_error_name(&e).to_string());
        if got != c.error {
            return Err(format!("stop canonical case {i}: {got:?}, the vector says {:?}", c.error));
        }
    }
    for (i, c) in file.matches.iter().enumerate() {
        let got = decode_stop_match_v4(&c.stop_sequences, &c.generated);
        if got != c.expected {
            return Err(format!("stop match case {i}: {got:?}, the vector says {:?}", c.expected));
        }
    }
    for (i, c) in file.first_stop.iter().enumerate() {
        let got = decode_first_stop_v4(&c.stop_sequences, &c.answer);
        if got != c.expected {
            return Err(format!("first-stop case {i}: {got:?}, the vector says {:?}", c.expected));
        }
    }
    Ok(file.canonical.len() + file.matches.len() + file.first_stop.len())
}

/// One selection entry point under test: `(config, sampling, generated_before, row, admitted)` →
/// the committed lane, or `None` for an empty admitted set.
pub type FpV4SelectFn<'a> =
    &'a dyn Fn(&DecodeConfigV4, &PalwDecodeSamplingV2, &[u32], &[i32], &dyn Fn(usize) -> bool) -> Option<usize>;

/// One decoding loop under test: `(config, limit, rows)` → `(the lanes fed to the engine, the
/// committed answer, the first stop)` — what an engine's decode loop does with a fixed row list.
pub type FpV4RunFn<'a> = &'a dyn Fn(&DecodeConfigV4, u32, &[Vec<i32>]) -> (Vec<u32>, Vec<u32>, Option<PalwFpDecodeStopV1>);

#[derive(Deserialize)]
struct ProcessorFile {
    cases: Vec<ProcessorCase>,
    decoder_runs: Vec<DecoderRun>,
}
#[derive(Deserialize)]
struct ProcessorCase {
    name: String,
    config: DecodeConfigV4,
    seed: String,
    temperature_q: u32,
    generated_before: Vec<u32>,
    row: Vec<i32>,
    admitted: Option<Vec<usize>>,
    processed: Vec<Option<i32>>,
    expected_lane: Option<usize>,
}
#[derive(Deserialize)]
struct DecoderRun {
    name: String,
    config: DecodeConfigV4,
    limit: u32,
    rows: Vec<Vec<i32>>,
    fed: Vec<u32>,
    generated: Vec<u32>,
    stop: Option<RunStop>,
}
#[derive(Deserialize)]
struct RunStop {
    executed: u32,
    reason: String,
}

/// `processor_order.json`: every lane's processed value (steps 1–4), the committed lane (5–6)
/// through `select`, and whole decode loops (7) through `run`.
pub fn fp_v4_check_processor_vectors(json: &str, select: FpV4SelectFn<'_>, run: FpV4RunFn<'_>) -> Result<usize, String> {
    let file: ProcessorFile = parse(json)?;
    for c in &file.cases {
        c.config.validate_canonical().map_err(|e| format!("{}: the vector's config is not canonical: {e}", c.name))?;
        for (lane, value) in c.row.iter().enumerate() {
            let count = decode_window_count_v4(&c.generated_before, c.config.penalty_window, lane as u32);
            let got = decode_lane_value_v4(&c.config, *value, lane as u32, count);
            if got != c.processed[lane] {
                return Err(format!("{}: lane {lane} processes to {got:?}, the vector says {:?}", c.name, c.processed[lane]));
            }
        }
        let sampling = sampling_of(&c.seed, c.temperature_q)?;
        let admitted = |lane: usize| c.admitted.as_ref().is_none_or(|set| set.contains(&lane));
        let got = select(&c.config, &sampling, &c.generated_before, &c.row, &admitted);
        if got != c.expected_lane {
            return Err(format!("{} (T {}): selects {got:?}, the vector says {:?}", c.name, c.temperature_q, c.expected_lane));
        }
    }
    for r in &file.decoder_runs {
        let (fed, generated, stop) = run(&r.config, r.limit, &r.rows);
        let stop = stop.map(|s| (s.executed, fp_v4_stop_name(s.reason)));
        let expected = r.stop.as_ref().map(|s| (s.executed, s.reason.clone()));
        if fed != r.fed || generated != r.generated || stop != expected {
            return Err(format!(
                "decoder run {}: fed {fed:?} / committed {generated:?} / stop {stop:?}, the vector says {:?} / {:?} / {expected:?}",
                r.name, r.fed, r.generated
            ));
        }
    }
    Ok(file.cases.len() + file.decoder_runs.len())
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

/// `v4_noop_equals_v3.json`: the V3 rule (`select_v3(sampling, position, row)`) and the no-op V4
/// (`select`) both commit the vector's lane (G7).
pub fn fp_v4_check_noop_vectors(
    json: &str,
    select_v3: &dyn Fn(&PalwDecodeSamplingV2, u32, &[i32]) -> usize,
    select: FpV4SelectFn<'_>,
) -> Result<usize, String> {
    let file: NoopFile = parse(json)?;
    for (i, c) in file.cases.iter().enumerate() {
        let sampling = sampling_of(&c.seed, c.temperature_q)?;
        let v3 = select_v3(&sampling, c.generated_before.len() as u32, &c.row);
        let v4 = select(&DecodeConfigV4::NOOP, &sampling, &c.generated_before, &c.row, &|_| true);
        if v3 != c.expected_lane || v4 != Some(c.expected_lane) {
            return Err(format!("no-op case {i}: V3 {v3}, V4 {v4:?}, the vector says {}", c.expected_lane));
        }
    }
    Ok(file.cases.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_decode_pipeline_v4::{PalwFpDecoderV1, decode_select_v4};

    const REPEAT: &str = include_str!("../../../consensus-vectors/fp-v4/repeat_penalty.json");
    const FREQUENCY: &str = include_str!("../../../consensus-vectors/fp-v4/frequency_penalty.json");
    const PRESENCE: &str = include_str!("../../../consensus-vectors/fp-v4/presence_penalty.json");
    const BIAS: &str = include_str!("../../../consensus-vectors/fp-v4/logit_bias.json");
    const STOP: &str = include_str!("../../../consensus-vectors/fp-v4/stop_sequences.json");
    const PROCESSOR: &str = include_str!("../../../consensus-vectors/fp-v4/processor_order.json");
    const NOOP: &str = include_str!("../../../consensus-vectors/fp-v4/v4_noop_equals_v3.json");

    /// The sampler's own decode loop over a row list: the decoder an engine drives.
    fn run(config: &DecodeConfigV4, limit: u32, rows: &[Vec<i32>]) -> (Vec<u32>, Vec<u32>, Option<PalwFpDecodeStopV1>) {
        let mut decoder = PalwFpDecoderV1::v4(config.clone(), PalwDecodeSamplingV2::GREEDY, limit);
        let fed = rows.iter().map(|row| decoder.select(row)).collect();
        (fed, decoder.generated().to_vec(), decoder.stop())
    }

    #[test]
    fn the_sampler_passes_every_golden_vector() {
        let n = [
            fp_v4_check_repeat_vectors(REPEAT),
            fp_v4_check_penalty_vectors(FREQUENCY),
            fp_v4_check_penalty_vectors(PRESENCE),
            fp_v4_check_logit_bias_vectors(BIAS),
            fp_v4_check_stop_vectors(STOP),
            fp_v4_check_processor_vectors(PROCESSOR, &decode_select_v4, &run),
            fp_v4_check_noop_vectors(
                NOOP,
                &|s, t, row| crate::palw_decode_select_v2::decode_token_select_v2(row, &s.seed, t, s.temperature_q),
                &decode_select_v4,
            ),
        ];
        let mut total = 0;
        for (file, result) in FP_V4_VECTOR_FILES.iter().zip(n) {
            let count = result.unwrap_or_else(|e| panic!("{file}: {e}"));
            assert!(count > 0, "{file} compared nothing");
            total += count;
        }
        assert!(total > 500, "the vectors are a sweep, not a sample ({total})");
    }

    #[test]
    fn a_vector_that_disagrees_is_named() {
        let wrong = r#"{"cases":[{"value":10,"count":1,"repeat_penalty_q":98304,"expected":7}]}"#;
        assert!(fp_v4_check_repeat_vectors(wrong).unwrap_err().contains("the vector says 7"));
    }
}
