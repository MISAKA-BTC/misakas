//! **Whisper — an encoder over FEATURE FRAMES, then a cross-attending decoder — against its Hugging Face fixture** (FR-18 phase 2
//! / FR-23's model stage; `tools/gen_hf_encdec_fixtures.py`). The adapter `whisper` is data only (there is no Rust reader of this
//! family): the encoder reads the normalised log-mel `[bins, 2·L]` as `i16` codes at 2^-13, through two `Conv1d` (the
//! convolution lowering of `lower/cnn.rs` with a kernel along the width) and their activation, adds the checkpoint's position
//! table, and runs pre-LN layers without a key bias; the decoder reads every one of the encoder's rows (a fixed 30 s window: no
//! count, no mask). For the tiny model:
//! 1. the float encoder and decoder against HF (the encoder's rows, the decoder's logits over HF's own stream and a random one);
//! 2. calibration on other random frames;
//! 3. the two programs — the encoder's `Final` is every decoder layer's cross K/V — run as version-2 programs, the decoder fed the
//!    INTEGER encoder's output, held against HF's logits (top-1, KL);
//! 4. the three implementations and the court's demand evaluator on both stages;
//! 5. admission of each stage (no pipeline: the protocol has no audio input binding yet — `JobAudio`, FR-23).
//! The real shapes (tiny … large-v3, no weights) are lowered and admitted or refused by name.

mod common;

use common::{court_coverage, int_tensor, three_ways, with_inputs};
use misaka_palw_tir as tir;
use misaka_palw_tir_lower::encoder;
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::hf_schema::{ReadOptions, read_encdec};
use misaka_palw_tir_lower::lower::encdec::{self, MEL_PARAM, MEL_Q, Source, Stats};
use misaka_palw_tir_lower::lower::{IntTensor, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::{Checkpoint, TensorSource};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf-encdec/whisper")
}

fn rows_of(v: &Value) -> Vec<Vec<f64>> {
    v.as_array().unwrap().iter().map(|r| r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()).collect()
}

fn ids_of(v: &Value) -> Vec<usize> {
    v.as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect()
}

fn rel(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f64>().sqrt() / b.iter().map(|x| x * x).sum::<f64>().sqrt()
}

fn argmax(v: &[f64]) -> usize {
    v.iter().enumerate().fold(0, |b, (i, x)| if *x > v[b] { i } else { b })
}

fn kl(p: &[f64], q: &[f64]) -> f64 {
    let lse = |v: &[f64]| {
        let m = v.iter().cloned().fold(f64::MIN, f64::max);
        m + v.iter().map(|x| (x - m).exp()).sum::<f64>().ln()
    };
    let (lp, lq) = (lse(p), lse(q));
    p.iter().zip(q).map(|(a, b)| (a - lp).exp() * ((a - lp) - (b - lq))).sum()
}

/// HF's `[bins][frames]` features as the program's `[frames][bins]` rows.
fn frames_of(mel: &Value) -> Vec<Vec<f64>> {
    let m = rows_of(mel);
    (0..m[0].len()).map(|t| m.iter().map(|b| b[t]).collect()).collect()
}

/// The frames as the program's input: `i16 [frames, bins]` codes at 2^-MEL_Q.
fn codes_of(frames: &[Vec<f64>]) -> IntTensor {
    let (t, b) = (frames.len(), frames[0].len());
    IntTensor::i16(vec![t, b], frames.iter().flatten().map(|v| (v * (1u64 << MEL_Q) as f64).round() as i16).collect())
}

#[test]
fn whisper_matches_its_hf_fixture() {
    use rand::{Rng, SeedableRng};
    let dir = fixture_dir();
    let cfg: Value = serde_json::from_slice(&std::fs::read(dir.join("config.json")).expect("config")).expect("json");
    let o: Value = serde_json::from_slice(&std::fs::read(dir.join("outputs.json")).expect("outputs")).expect("json");
    let read = read_encdec(&cfg, &ReadOptions::default()).unwrap_or_else(|e| panic!("the adapter: {e}"));
    assert!(matches!(&read.adapter, misaka_palw_tir_lower::hf_schema::AdapterSource::BuiltIn { id, .. } if id == "whisper"), "{:?}", read.adapter);
    let s = read.spec;
    let lmax = (s.enc_pos_rows.unwrap()) as u32;
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let names: std::collections::BTreeSet<String> = ck.names().into_iter().collect();
    let has = |n: &str| names.contains(n);
    let ((ehl, ebind), (dhl, dbind)) = encdec::hl_programs(&s, lmax as usize, &has).expect("hl programs");
    let (ep, e_unread) = ParamStore::from_source(&ehl, &ebind, &ck).expect("encoder params");
    let (dp, d_unread) = ParamStore::from_source(&dhl, &dbind, &ck).expect("decoder params");
    let unread: Vec<&String> = e_unread.iter().filter(|n| d_unread.contains(n)).collect();
    assert!(unread.is_empty(), "checkpoint tensors neither stage reads: {unread:?}");
    // 1. The float stages are HF's.
    let recs = o["records"].as_array().unwrap();
    for rec in recs {
        let frames = frames_of(&rec["input_features"]);
        let enc = encdec::float_encoder_src(&ehl, &s, &ep, Source::Frames(&frames), None).expect("float encoder");
        let r = rel(&enc.hidden.concat(), &rows_of(&rec["encoder_hidden"]).concat());
        assert!(r < 1e-4, "float encoder vs HF rel {r}");
        for (ids_key, logits_key) in [("decoder_input_ids", "logits"), ("random_decoder_ids", "random_logits")] {
            let lg = encdec::float_decoder(&dhl, &s, &dp, &enc, &ids_of(&rec[ids_key]), None).expect("float decoder");
            let r2 = rel(&lg.concat(), &rows_of(&rec[logits_key]).concat());
            eprintln!("whisper float vs HF: encoder rows rel {r:.2e}, decoder `{logits_key}` rel {r2:.2e}");
            assert!(r2 < 1e-4, "float decoder vs HF `{logits_key}` rel {r2}");
        }
    }
    // 2. Calibration: random frames (in a normalised log-mel's range, on the code grid) and random streams.
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(7);
    let (mut es, mut ds): (Stats, Stats) = (Stats::new(), Stats::new());
    let (bins, frames_n) = (cfg["num_mel_bins"].as_u64().unwrap() as usize, 2 * lmax as usize);
    for _ in 0..10 {
        let f: Vec<Vec<f64>> = (0..frames_n).map(|_| (0..bins).map(|_| ((rng.gen_range(-1.0..1.5f64)) * 8192.0).round() / 8192.0).collect()).collect();
        let enc = encdec::float_encoder_src(&ehl, &s, &ep, Source::Frames(&f), Some(&mut es)).expect("calibration encoder");
        let m = rng.gen_range(4..=8usize);
        let stream: Vec<usize> = std::iter::once(s.decoder_start as usize).chain((1..m).map(|_| rng.gen_range(0..s.vocab))).collect();
        encdec::float_decoder(&dhl, &s, &dp, &enc, &stream, Some(&mut ds)).expect("calibration decoder");
    }
    // 3. The two programs.
    let quiet = |_: usize, _: usize| {};
    let elw = encdec::lower_encoder(&ehl, &s, lmax).expect("lower encoder");
    let dlw = encdec::lower_decoder(&dhl, &s, lmax, 32).expect("lower decoder");
    let emat = materialise(&elw, &ehl, &Resident(Arc::new(ep)), &es, &QuantPolicy::default(), &quiet).expect("materialise encoder");
    let dmat = materialise(&dlw, &dhl, &Resident(Arc::new(dp)), &ds, &QuantPolicy::default(), &quiet).expect("materialise decoder");
    let e2 = encoder::encdec_frames_encoder_v2(&elw).expect("encoder v2");
    let d2 = encoder::encdec_fixed_decoder_v2(&dlw).expect("decoder v2");
    let e_params = encoder::lifted_params(&elw.program, &[MEL_PARAM], &emat.params);
    let d_params = encoder::lifted_params(&dlw.program, &[encdec::XKV_PARAM], &dmat.params);
    let einterp = tir::interp_v2::InterpreterV2::new(&e2).expect("encoder interpreter");
    let dinterp = tir::interp_v2::InterpreterV2::new(&d2).expect("decoder interpreter");
    let (mut agree, mut total, mut kl_sum) = (0usize, 0usize, 0f64);
    let mut first = None;
    for rec in recs {
        let frames = frames_of(&rec["input_features"]);
        let mel = codes_of(&frames);
        let mut ein = tir::interp_v2::MapInputs::default();
        ein.constant.insert(0, mel.to_tir());
        let xkv = einterp.run_positions(&e_params, &ein, 1).expect("the integer encoder").remove(0).output;
        let mut din = tir::interp_v2::MapInputs::default();
        din.constant.insert(0, xkv.clone());
        for (ids_key, logits_key) in [("decoder_input_ids", "logits"), ("random_decoder_ids", "random_logits")] {
            let stream: Vec<u32> = ids_of(&rec[ids_key]).iter().map(|t| *t as u32).collect();
            let run = dinterp.run(&d_params, &din, &stream).expect("the integer decoder");
            let want = rows_of(&rec[logits_key]);
            let got: Vec<Vec<f64>> = run.iter().map(|st| st.output.data.iter().map(|c| *c as f64 * dmat.logits_scale).collect()).collect();
            let r = rel(&got.concat(), &want.concat());
            let hits = got.iter().zip(&want).filter(|(a, b)| argmax(a) == argmax(b)).count();
            let k: f64 = got.iter().zip(&want).map(|(a, b)| kl(b, a)).sum();
            eprintln!("whisper integer vs HF `{logits_key}`: logits rel {r:.2e}, top-1 {hits}/{}, mean KL {:.2e}", want.len(), k / want.len() as f64);
            assert!(r < 0.05, "logits rel {r}");
            agree += hits;
            total += want.len();
            kl_sum += k;
        }
        first.get_or_insert((mel, xkv));
    }
    assert!(agree as f64 / total as f64 >= 0.85 && kl_sum / (total as f64) < 0.05, "top-1 {agree}/{total}, KL {}", kl_sum / total as f64);
    // 4. The three implementations and the court, on both stages (the lowering's inputs are params in the version-1 view; the
    //    decoder is fed the integer encoder's own cross K/V).
    {
        let (mel, xkv) = first.expect("a record");
        let ep6 = with_inputs(&elw.program, &emat.params, &[(MEL_PARAM, mel)]);
        let dp6 = with_inputs(&dlw.program, &dmat.params, &[(encdec::XKV_PARAM, int_tensor(&xkv))]);
        let stream: Vec<u32> = ids_of(&recs[0]["decoder_input_ids"]).iter().map(|t| *t as u32).collect();
        let last = stream.len() as u32 - 1;
        let n3 = three_ways(&elw.program, &ep6, &[vec![0]]).unwrap_or_else(|e| panic!("encoder, three implementations: {e}"));
        let n3d = three_ways(&dlw.program, &dp6, &[stream.iter().map(|t| *t as usize).collect()]).unwrap_or_else(|e| panic!("decoder, three implementations: {e}"));
        let ce = court_coverage(&elw.program, &ep6, &[0], &[0], &[1]).unwrap_or_else(|e| panic!("encoder, court: {e}"));
        let cd = court_coverage(&dlw.program, &dp6, &stream, &[0, 1, last], &[1, 3]).unwrap_or_else(|e| panic!("decoder, court: {e}"));
        eprintln!(
            "whisper COURT: three implementations equal at {n3} + {n3d} positions; the court replays {} + {} commit points ({} + {} nodes) ",
            ce.commits, cd.commits, ce.nodes, cd.nodes
        );
        assert!(ce.commits > 0 && cd.commits > 0);
    }
    // 5. Admission of each stage.
    let inputs = misaka_palw_tir_lower::admission::default_inputs();
    let ea = tir::admit_v2::tir_admit_program_v2(&e2, &inputs).expect("the encoder is admitted");
    let da = tir::admit_v2::tir_admit_program_v2(&d2, &inputs).expect("the decoder is admitted");
    eprintln!(
        "whisper admitted: encoder {} nodes, {:?}; decoder {} nodes, {:?}",
        e2.blocks.iter().map(|b| b.nodes.len()).sum::<usize>(),
        ea.view.position,
        d2.blocks.iter().map(|b| b.nodes.len()).sum::<usize>(),
        da.view.position
    );
}

/// The model's refusals are named: a source whose frames the stem does not map onto the table's rows, a table too short for
/// the source, and an adapter that is read only for Whisper's own architecture.
#[test]
fn the_frame_encoders_geometry_is_checked_by_name() {
    let dir = fixture_dir();
    let cfg: Value = serde_json::from_slice(&std::fs::read(dir.join("config.json")).unwrap()).unwrap();
    let s = read_encdec(&cfg, &ReadOptions::default()).unwrap().spec;
    let ck = Checkpoint::open(&dir).unwrap();
    let names: std::collections::BTreeSet<String> = ck.names().into_iter().collect();
    let has = |n: &str| names.contains(n);
    // The frames give 16 rows; asking for 15 is refused by name.
    let ((ehl, _), _) = encdec::hl_programs(&s, 15, &has).unwrap();
    let e = encdec::lower_encoder(&ehl, &s, 15).err().expect("refused").to_string();
    assert!(e.contains("source rows, not 15"), "{e}");
    // A position table shorter than the source.
    let mut short = s.clone();
    short.enc_pos_rows = Some(8);
    let ((ehl, _), _) = encdec::hl_programs(&short, 16, &has).unwrap();
    let e = encdec::lower_encoder(&ehl, &short, 16).err().expect("refused").to_string();
    assert!(e.contains("needs positions up to"), "{e}");
    // A stem that is not named for every convolution.
    let mut bad = s.clone();
    bad.names.enc.stem.pop();
    assert!(bad.validate().unwrap_err().to_string().contains("stem"), "a stem convolution without a name");
}

/// **Real shapes** (`tests/configs/encdec/whisper-*.json`, no weights): both stages lower at the model's own 1,500 source rows
/// and are admitted at the legacy court's ceilings, or refused by name — the numbers are the program's, whatever the weights.
#[test]
fn real_whispers_lower_and_are_admitted_or_refused_by_name() {
    for name in ["whisper-tiny", "whisper-base", "whisper-small", "whisper-medium", "whisper-large-v3"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/encdec").join(format!("{name}.json"));
        let cfg: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("config")).expect("json");
        let s = read_encdec(&cfg, &ReadOptions::default()).unwrap_or_else(|e| panic!("{name}: the adapter: {e}")).spec;
        let l = s.enc_pos_rows.unwrap() as u32;
        let ((ehl, _), (dhl, _)) = encdec::hl_programs(&s, l as usize, &|_| false).expect("hl programs");
        let elw = encdec::lower_encoder(&ehl, &s, l).expect("lower encoder");
        let dlw = encdec::lower_decoder(&dhl, &s, l, 448).expect("lower decoder");
        let e2 = encoder::encdec_frames_encoder_v2(&elw).expect("encoder v2");
        let d2 = encoder::encdec_fixed_decoder_v2(&dlw).expect("decoder v2");
        let nodes = |p: &tir::program_v2::TirProgramV2| -> (usize, usize) {
            (p.blocks.iter().map(|b| b.nodes.len()).sum(), p.blocks.iter().map(|b| b.nodes.len()).max().unwrap_or(0))
        };
        let inputs = misaka_palw_tir_lower::admission::default_inputs();
        let verdict = |p: &tir::program_v2::TirProgramV2| match tir::admit_v2::tir_admit_program_v2(p, &inputs) {
            Ok(a) => format!("ADMITTED (position {:.3e} MACs, {} step leaves)", a.view.position.cost.macs as f64, a.view.position.step_leaves),
            Err(e) => format!("REFUSED: {e}"),
        };
        let ((en, em), (dn, dm)) = (nodes(&e2), nodes(&d2));
        eprintln!("{name} at {l} source rows: encoder {en} nodes (max {em}/block) {}; decoder {dn} nodes (max {dm}/block) {}", verdict(&e2), verdict(&d2));
        // The decoder is a text stage of one position over a fixed source axis: admitted at every size.
        assert!(tir::admit_v2::tir_admit_program_v2(&d2, &inputs).is_ok(), "{name}: the decoder is refused");
    }
}

/// **The levers for the sizes the default ceilings refuse** (measured, no model change): the admission input `tile_len` — the values a
/// step leaf holds (64 in `default_inputs`; the class declares it) — divides the leaf count of the 1,500-frame attention's committed
/// logits, and the encoder of whisper-small is ADMITTED at 256 (2.44 M leaves of 4.19 M). The longest useful tile is bounded by the
/// court's per-tile exponentials (a tile of the softmax recomputes its rows: `max_tile_transcendentals`, 1.54 M at 512 against 1.05 M),
/// about 340: whisper-medium would still have 4.8 M leaves there, so it needs the committed values themselves reduced (a stat-only
/// softmax, `ATTN_STAT_COMMIT_V1`, or an encoder cut across stage programs); whisper-large-v3 is refused by `max_position_macs` at every
/// tile — a position is the whole encoder, and only a stage split brings it under 2^40.
#[test]
fn a_longer_tile_admits_whisper_small_and_the_larger_sizes_need_stages() {
    let encoder_at = |name: &str, tile: u32| -> Result<(u64, u64), String> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/encdec").join(format!("{name}.json"));
        let cfg: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("config")).expect("json");
        let s = read_encdec(&cfg, &ReadOptions::default()).expect("adapter").spec;
        let l = s.enc_pos_rows.unwrap() as u32;
        let ((ehl, _), _) = encdec::hl_programs(&s, l as usize, &|_| false).expect("hl programs");
        let elw = encdec::lower_encoder(&ehl, &s, l).expect("lower encoder");
        let e2 = encoder::encdec_frames_encoder_v2(&elw).expect("encoder v2");
        let mut inputs = misaka_palw_tir_lower::admission::default_inputs();
        inputs.tile_len = tile;
        tir::admit_v2::tir_admit_program_v2(&e2, &inputs).map(|a| (a.view.position.step_leaves, a.view.position.cost.macs)).map_err(|e| e.to_string())
    };
    // tiny and base are admitted at the default tile and at 256.
    for name in ["whisper-tiny", "whisper-base"] {
        assert!(encoder_at(name, 64).is_ok() && encoder_at(name, 256).is_ok(), "{name}");
    }
    // small: refused at 64 and 128 by the leaf cap, admitted at 256, refused at 512 by the court's exponentials per tile.
    assert!(encoder_at("whisper-small", 64).unwrap_err().contains("max_step_leaves 9749268"));
    assert!(encoder_at("whisper-small", 128).unwrap_err().contains("max_step_leaves 4874640"));
    let (leaves, _) = encoder_at("whisper-small", 256).expect("whisper-small is admitted at a tile of 256");
    assert_eq!(leaves, 2_437_332);
    assert!(encoder_at("whisper-small", 512).unwrap_err().contains("max_tile_transcendentals 1537024"));
    // medium: the leaf cap at every tile the exponentials allow; large-v3: the position's MACs at every tile.
    for tile in [64, 128, 256] {
        assert!(encoder_at("whisper-medium", tile).unwrap_err().contains("max_step_leaves"), "medium at {tile}");
    }
    for tile in [64, 256, 4096] {
        assert!(encoder_at("whisper-large-v3", tile).unwrap_err().contains("max_position_macs 1294172160000"), "large-v3 at {tile}");
    }
}
