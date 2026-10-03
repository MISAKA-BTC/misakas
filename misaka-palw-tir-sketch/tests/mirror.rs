//! **The witness codec and the mirror check** (RFC-0007 Part II): a witness round-trips through its canonical image, a hostile image is
//! refused by name, chunks reassemble only when whole and true, and the mirror's agreement table holds on a real checker — an honest
//! own witness agrees, a lie in a served witness is refused (and agrees with a replay that does not reproduce it), and a swapped chunk
//! cannot be reassembled.
//!
//! Run: `cargo test -p misaka-palw-tir-sketch --test mirror`

use misaka_palw_tir_exec::TirPlan;
use misaka_palw_tir_sketch::codec;
use misaka_palw_tir_sketch::fixture::dense_moe_v1;
use misaka_palw_tir_sketch::*;

const CLASS: [u8; 64] = [0x52; 64];
const JOB_ID: [u8; 32] = [0x7B; 32];

fn job() -> TirSketchJobV1 {
    TirSketchJobV1 { prompt: vec![3, 17, 5, 29, 11], decode: 3 }
}

fn policy() -> TirCheckPolicyV1 {
    TirCheckPolicyV1 { act_act: TirActActPolicyV1::Served, weight_min_k: 0 }
}

struct Rig {
    plan: TirPlan,
    analysis: TirSketchAnalysisV1,
    fx: misaka_palw_tir_sketch::fixture::TirSketchFixtureV1,
}

fn rig() -> Rig {
    let fx = dense_moe_v1(8);
    let plan = TirPlan::compile(&fx.program).expect("a program in normal form");
    let analysis = TirSketchAnalysisV1::of(&fx.program);
    Rig { plan, analysis, fx }
}

impl Rig {
    fn honest(&self) -> TirWitnessV1 {
        tir_witness_produce_v1(&self.plan, &self.analysis, &self.fx.params, &job(), &policy(), &mut |_, _| {}).expect("the producer runs")
    }

    fn mirror(&self, w: &TirWitnessV1) -> TirMirrorOutcomeV1 {
        let keys = TirSeatSketchSecretV1::from_bytes([0x33; 32]).keys(&CLASS, 1);
        let store = TirSketchStoreV1::build(&self.plan, &self.analysis, &self.fx.params, &keys).expect("the store builds");
        tir_mirror_check_v1(&self.plan, &self.analysis, &store, &keys, &self.fx.params, &job(), &JOB_ID, w, policy()).expect("a check")
    }
}

#[test]
fn a_witness_round_trips_through_its_canonical_image_and_a_hostile_image_is_refused_by_name() {
    let r = rig();
    let w = r.honest();
    let image = codec::encode(&w);
    let back = codec::decode(&image).expect("a witness image decodes");
    assert_eq!(back, w, "the image round-trips");
    assert_eq!(codec::encode(&back), image, "and is canonical");
    assert!(image.len() as u64 >= w.served().1, "the image holds at least the served bytes");
    // Truncated at every length: an error, never a panic.
    for cut in (0..image.len()).step_by(97) {
        assert!(codec::decode(&image[..cut]).is_err(), "truncated at {cut}");
    }
    // Trailing bytes, a wrong version, a count the image cannot pay for, a dtype that is not one.
    let mut long = image.clone();
    long.push(0);
    assert!(codec::decode(&long).unwrap_err().contains("follow the witness image"));
    let mut version = image.clone();
    version[0] = 9;
    assert!(codec::decode(&version).unwrap_err().contains("version"));
    let mut huge = image.clone();
    huge[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(codec::decode(&huge).unwrap_err().contains("cannot fit"), "{:?}", codec::decode(&huge));
    assert!(codec::decode(&[]).is_err() && codec::decode(&[1, 0]).is_err());
}

#[test]
fn chunks_reassemble_only_when_whole_and_true() {
    let r = rig();
    let image = codec::encode(&r.honest());
    let cuts = codec::chunks(&image, 4_096);
    assert!(cuts.len() > 2, "{} chunks", cuts.len());
    let digests: Vec<[u8; 32]> = cuts.iter().enumerate().map(|(i, c)| codec::chunk_digest(i as u32, c)).collect();
    let served: Vec<(u32, Vec<u8>)> = cuts.iter().enumerate().map(|(i, c)| (i as u32, c.to_vec())).collect();
    // Any order is fine.
    let mut shuffled = served.clone();
    shuffled.reverse();
    assert_eq!(codec::assemble(shuffled, &digests), Some(image.clone()));
    // A missing chunk (the one a seat names `Unavailable`), a flipped byte, a swapped pair: refused.
    let mut missing = served.clone();
    missing.remove(1);
    assert!(codec::assemble(missing, &digests).is_none(), "a withheld chunk");
    let mut flipped = served.clone();
    flipped[1].1[0] ^= 1;
    assert!(codec::assemble(flipped, &digests).is_none(), "a flipped byte");
    let mut swapped = served.clone();
    let (a, b) = (swapped[0].1.clone(), swapped[1].1.clone());
    swapped[0].1 = b;
    swapped[1].1 = a;
    assert!(codec::assemble(swapped, &digests).is_none(), "a swapped pair: the digest binds the index");
    let mut extra = served.clone();
    extra.push((cuts.len() as u32, vec![1]));
    assert!(codec::assemble(extra, &digests).is_none(), "an extra chunk");
    assert!(codec::chunks(&image, 0).is_empty() && codec::chunks(&[], 4).is_empty());
}

#[test]
fn an_image_splits_into_exactly_the_pinned_number_of_chunks() {
    let r = rig();
    let image = codec::encode(&r.honest());
    for count in [1usize, 2, 3, 7, 50, image.len() + 5] {
        let parts = codec::split_into(&image, count);
        assert_eq!(parts.len(), count, "exactly the pinned count");
        assert_eq!(parts.concat(), image, "and they are the image");
    }
    assert!(codec::split_into(&image, 0).is_empty());
    assert_eq!(codec::split_into(&[], 3), vec![&[][..], &[][..], &[][..]], "an empty image is `count` empty chunks");
}

#[test]
fn the_mirror_agrees_on_an_honest_own_witness_and_refuses_a_lie() {
    let r = rig();
    let honest = r.honest();
    let out = r.mirror(&honest);
    assert!(out.accepted && out.report.is_some(), "the checker accepts the seat's own honest witness: {:?}", out.failure);
    assert_eq!(tir_mirror_agreement_v1(false, true, out.accepted), TirMirrorAgreementV1::Agree);
    // Through the wire: the same verdict on the decoded image.
    let decoded = codec::decode(&codec::encode(&honest)).unwrap();
    assert!(r.mirror(&decoded).accepted, "the decoded witness is the witness");

    // A producer that lies about one accumulator and computes on honestly from it: refused (and the replay would not reproduce its claim).
    let mut lied = false;
    let liar = tir_witness_produce_v1(&r.plan, &r.analysis, &r.fx.params, &job(), &policy(), &mut |site, value| {
        if !lied && site.pos == 2 && site.occurrence == 1 && value.dtype == misaka_palw_tir::DType::I64 && !value.data.is_empty() {
            value.data[0] += 1;
            lied = true;
        }
    })
    .expect("a lying producer still produces");
    assert!(lied, "the lie was placed");
    let refused = r.mirror(&liar);
    assert!(!refused.accepted && refused.failure.is_some(), "the checker refuses the lie: {refused:?}");
    assert_eq!(tir_mirror_agreement_v1(true, false, refused.accepted), TirMirrorAgreementV1::Agree, "a served lie the replay does not reproduce");
    assert_eq!(tir_mirror_agreement_v1(true, true, refused.accepted), TirMirrorAgreementV1::WitnessRefused, "an honest claim with a bad witness");
}

#[test]
fn the_agreement_table_alarms_exactly_on_a_false_reject_and_a_false_accept() {
    use TirMirrorAgreementV1::*;
    for served in [false, true] {
        for reproduces in [false, true] {
            for accepted in [false, true] {
                let a = tir_mirror_agreement_v1(served, reproduces, accepted);
                let alarm = (!served && !accepted) || (served && !reproduces && accepted);
                assert_eq!(a.is_alarm(), alarm, "served={served} reproduces={reproduces} accepted={accepted}: {a:?}");
            }
        }
    }
    assert_eq!(tir_mirror_agreement_v1(false, false, false), FalseReject);
    assert_eq!(tir_mirror_agreement_v1(true, false, true), FalseAccept);
    let mut totals = TirMirrorTotalsV1::default();
    totals.note(Agree, 10);
    totals.note(FalseAccept, 5);
    totals.note(WitnessRefused, 1);
    assert_eq!((totals.checked, totals.agreed, totals.false_accepts, totals.witness_refused, totals.served_bytes), (3, 1, 1, 1, 16));
    assert!(totals.status().contains("sketch_mirror_false_accepts=1"));
}
