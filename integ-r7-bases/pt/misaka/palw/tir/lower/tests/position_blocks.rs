//! **Runs of positions** (`Session::eval_occurrence_run`, `run_layer_major_keep`): the float reference evaluated node by node
//! over runs of positions — each linear's matrix read once per run, the 32K calibration's cost — gives the position-by-position
//! result BIT FOR BIT: the statistics' canonical JSON (whose digest a pack pins) and every logit, for each genesis
//! architecture's toy checkpoint, with run lengths that do and do not divide the sequence, past the sliding windows. A block
//! the run cannot take (a Mamba scan here) falls back to position by position, with the same result. Keeping a few positions'
//! logits keeps exactly those rows.
//!
//! **Consumed carries** (RFC-0020 §3, the user's `reduce_calibration_carries` patch adapted to runs): each incoming carry row is
//! retired as it is read instead of holding the input and the output sequences at once — the statistics' digest and every
//! logit equal the layer-major run as it was before (two full sequences, position by position), verbatim below; and with the
//! carries **spilled to disk** (RFC-0020 §6) they equal it too.

use misaka_palw_tir_lower::calib::{stats_digest_hex, stats_to_json};
use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::float_ref::stream::{KeepLogits, OccParams, Resident, SpillCfg, run_layer_major_keep, set_carry_spill};
use misaka_palw_tir_lower::float_ref::{ParamStore, Session, SiteStat, block_runs_by_position};
use misaka_palw_tir_lower::hl::HlProgram;
use misaka_palw_tir_lower::lower::LowerOpts;
use misaka_palw_tir_lower::weights::Checkpoint;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(rel)
}

/// `(fixture, whether some layer block falls back to position by position)`.
const FIXTURES: &[(&str, bool)] = &[
    ("hf/qwen2_sliding", false),
    ("hf/qwen3_5", false),
    ("hf/qwen3_5_moe", false),
    ("genesis8-tiny/gemma4_unified", false),
    ("hf/gpt_oss", false),
    ("hf/llama", false),
    ("hf/jamba", true),
];

fn bits(rows: &[Vec<f32>]) -> Vec<Vec<u32>> {
    rows.iter().map(|r| r.iter().map(|v| v.to_bits()).collect()).collect()
}

#[test]
fn runs_of_positions_are_the_position_by_position_result_bit_for_bit() {
    let quiet = |_: usize, _: usize| {};
    for (rel, falls_back) in FIXTURES {
        let dir = fixture(rel);
        let cfg = std::fs::read_to_string(dir.join("config.json")).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let prep = fidelity::prepare(&cfg, &LowerOpts::default()).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let ck = Checkpoint::open(&dir).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let (store, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let loader = Resident(Arc::new(store));
        let layers = || prep.hl.schedule.iter().map(|k| block_runs_by_position(&prep.hl, *k as usize));
        if *falls_back {
            assert!(layers().any(|runs| !runs), "{rel}: a Mamba block runs position by position");
        } else {
            assert!(layers().any(|runs| runs), "{rel}: the layer blocks run node by node");
        }
        // Three sequences of 37 positions: runs of 2, 5, 32 and 64 leave a remainder or cover a sequence in one run, and every
        // sliding window (4 in these fixtures) is passed many times over.
        let seqs = fidelity::random_sequences(prep.hl.vocab, 3, 37, 11);
        let one = run_layer_major_keep(&prep.hl, &loader, &seqs, true, &KeepLogits::All, 1, &quiet)
            .unwrap_or_else(|e| panic!("{rel}: position by position: {e}"));
        assert_eq!(one.logits[0].len(), 37);
        for block in [2, 5, 32, 64] {
            let got = run_layer_major_keep(&prep.hl, &loader, &seqs, true, &KeepLogits::All, block, &quiet)
                .unwrap_or_else(|e| panic!("{rel}: runs of {block}: {e}"));
            assert_eq!(stats_to_json(&got.stats), stats_to_json(&one.stats), "{rel}: the statistics with runs of {block}");
            for (si, (a, b)) in got.logits.iter().zip(&one.logits).enumerate() {
                assert_eq!(bits(a), bits(b), "{rel}: sequence {si}'s logits with runs of {block}");
            }
        }
        // A long evaluation keeps a few positions' rows only.
        let at: BTreeSet<usize> = [0, 7, 36].into_iter().collect();
        let some = run_layer_major_keep(&prep.hl, &loader, &seqs, false, &KeepLogits::At(at), 5, &quiet)
            .unwrap_or_else(|e| panic!("{rel}: kept positions: {e}"));
        assert!(some.stats.is_empty());
        for (si, rows) in some.logits.iter().enumerate() {
            assert_eq!(some.logit_positions[si], vec![0, 7, 36], "{rel}");
            for (k, p) in some.logit_positions[si].iter().enumerate() {
                assert_eq!(bits(&rows[k..k + 1]), bits(&one.logits[si][*p..*p + 1]), "{rel}: sequence {si} position {p}");
            }
        }
    }
}

/// The layer-major run as it was before RFC-0020 §3: the incoming and the outgoing carry sequences held at once, every position
/// evaluated one at a time, sequences merged in order — verbatim but for the sequences running one after another (their
/// statistics were merged in sequence order then too).
fn layer_major_before(
    prog: &HlProgram,
    loader: &dyn OccParams,
    seqs: &[Vec<usize>],
) -> (BTreeMap<String, SiteStat>, Vec<Vec<Vec<f32>>>) {
    let mut occs: Vec<(usize, Option<usize>)> = vec![(prog.pre, None)];
    occs.extend(prog.schedule.iter().enumerate().map(|(l, k)| (*k as usize, Some(l))));
    occs.push((prog.post, None));
    let mut carries: Vec<Vec<Vec<Vec<f32>>>> = seqs.iter().map(|s| vec![Vec::new(); s.len()]).collect();
    let (mut stats, mut logits) = (BTreeMap::<String, SiteStat>::new(), vec![Vec::new(); seqs.len()]);
    for (bi, layer) in occs {
        let store = loader.load(bi, layer).expect("load");
        let mut next = Vec::with_capacity(seqs.len());
        for (si, toks) in seqs.iter().enumerate() {
            let mut sess = Session::new(prog, &store).with_site_stats();
            let mut outs = Vec::with_capacity(toks.len());
            for (p, t) in toks.iter().enumerate() {
                outs.push(sess.eval_occurrence(bi, layer, &carries[si][p], *t, p).expect("evaluate"));
            }
            if bi == prog.post {
                logits[si] = outs.into_iter().map(|mut o| o.swap_remove(0)).collect();
                next.push(Vec::new());
            } else {
                next.push(outs);
            }
            for (k, v) in sess.sites.take().unwrap_or_default() {
                stats.entry(k).or_default().merge(&v);
            }
        }
        carries = next;
    }
    (stats, logits)
}

#[test]
fn consumed_and_spilled_carries_are_the_two_sequence_run_bit_for_bit() {
    let quiet = |_: usize, _: usize| {};
    let spill_dir = std::env::temp_dir().join(format!("palw-carry-spill-{}", std::process::id()));
    for (rel, _) in FIXTURES {
        let dir = fixture(rel);
        let cfg = std::fs::read_to_string(dir.join("config.json")).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let prep = fidelity::prepare(&cfg, &LowerOpts::default()).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let ck = Checkpoint::open(&dir).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let (store, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let loader = Resident(Arc::new(store));
        let seqs = fidelity::random_sequences(prep.hl.vocab, 2, 29, 5);
        let (want_stats, want_logits) = layer_major_before(&prep.hl, &loader, &seqs);
        let check = |label: &str, block: usize| {
            let got = run_layer_major_keep(&prep.hl, &loader, &seqs, true, &KeepLogits::All, block, &quiet)
                .unwrap_or_else(|e| panic!("{rel} {label}: {e}"));
            assert_eq!(stats_digest_hex(&got.stats), stats_digest_hex(&want_stats), "{rel} {label}: the statistics digest");
            assert_eq!(stats_to_json(&got.stats), stats_to_json(&want_stats), "{rel} {label}: the statistics");
            for (si, (a, b)) in got.logits.iter().zip(&want_logits).enumerate() {
                assert_eq!(bits(a), bits(b), "{rel} {label}: sequence {si}'s logits");
            }
        };
        check("consumed carries, position by position", 1);
        check("consumed carries, runs of 8", 8);
        // Every occurrence's carries on disk (budget 0), then back in RAM.
        set_carry_spill(Some(SpillCfg { dir: spill_dir.clone(), budget_bytes: 0 }));
        check("spilled carries, runs of 8", 8);
        set_carry_spill(None);
    }
}
