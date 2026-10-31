//! **Agent C4 — adversarial wire fuzz of the kernel route** (independent of the lane B/D suites).
//!
//! Attack: an unauthenticated stranger with ONE bonded identity (free collateral for a filing fee, nothing else) hands the fold
//! encoded route objects whose bytes are mutated: byte flips, truncations, length-prefix lies and — the structured part — tensor
//! shapes / dtypes / openings with extreme values inside otherwise valid proofs, responses and registrations.
//!
//! Invariants asserted for EVERY mutant (each is applied to a fresh clone of the same ledger through the strict decoder):
//!   I1  the fold never panics (a panic in a consensus fold halts every node that processes the block);
//!   I2  a refusal leaves the state root byte-identical;
//!   I3  an honest claim is never convicted by anything a stranger can encode;
//!   I4  every settlement instruction an applied mutant emits is applicable to an independent bond book, slashes are routed 1:1
//!       to reward + burn, value is conserved, and the ledger's view of every bond equals the book's.
//! Expected BEFORE running: I1 may fail on arithmetic over attacker-controlled shapes (`element_count` / `LayoutV1::of` use
//! unchecked products and the release profile has `overflow-checks = true`, so an overflow is a panic, not a wrap).

#![allow(warnings)]
mod common;
#[path = "adv_c4_support/mod.rs"]
mod support;

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;

use common::chain::T;
use common::ledger_world::*;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::DecodeFaultV1;
use misaka_palw_kernel::ledger::{
    AuthV1, KernelLedgerV1, KernelRouteObjectV1 as O, LedgerEventV1 as E, OutsiderFindingV1, ProsecutionV1,
};
use misaka_palw_kernel::merkle::TensorOpeningV1;
use misaka_palw_kernel::public::{FaultProofWireV1, MaterialResponseV1, PositionResponseV1, TensorWireV1};
use support::Book;

static PANICS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

fn install_hook() {
    std::panic::set_hook(Box::new(|info| {
        let bt = std::backtrace::Backtrace::force_capture().to_string();
        // The first workspace frame above the panic: who called the arithmetic.
        let caller = bt
            .lines()
            .filter(|l| l.contains("misaka_palw") && !l.contains("adv_c4_fuzz") && !l.contains("{{closure}}"))
            .map(|l| l.trim().to_string())
            .next()
            .unwrap_or_default();
        let loc = format!("{} <- {}", info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default(), caller);
        let msg =
            info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned());
        PANICS.lock().unwrap().push((loc, msg.unwrap_or_default()));
    }));
}

struct Fixture {
    ledger: KernelLedgerV1,
    book: Book,
    honest: Digest,
    lying: Digest,
    samples: Vec<(&'static str, O, Digest)>,
}

fn fixture() -> Fixture {
    let mut w = World::new();
    let job1 = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job1, 3);
    let (hid, hda) = (h.claim.id(), Da::publishing(&h.trace, &[]));
    w.block(10, vec![h.tx.clone(), T::PanelCovered { claim: hid }]);
    let job2 = w.post_job(11, &[3, 17, 9], 3, 2);
    let (_at, lie) = w.lying(&job2, 3);
    let (lid, lda) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(12, vec![lie.tx.clone(), T::PanelCovered { claim: lid }]);
    w.block(13, vec![T::FileDemand { demander: OUTSIDER, claim: lid, stage: 0, position: 1 }]);

    let OutsiderFindingV1::Prosecute(ProsecutionV1::Kernel(proof)) = outsider(&w, lid, &lda) else { panic!("the lie is found") };
    let (post, logits) = w.l.classes[&w.class].logits_at();
    let honest_logits = hda.get(2, post, logits).unwrap();
    let decode = DecodeFaultV1 { index: 0, logits: TensorWireV1::of(&honest_logits) };
    let resp = position(&lie.trace, 1, |_| {});
    let job3 = KernelJobLike::job(&w, 3);
    let h3 = w.honest(&job3, 3);
    let T::CommitClaim { claim, evidence, commitments } = h3.tx.clone() else { unreachable!() };
    // The sample claim's seal is in the ledger (a CommitClaim without one is refused before anything interesting happens).
    w.block(14, common::chain::seals_for(std::slice::from_ref(&h3.tx)));
    let register = {
        let T::RegisterClass { descriptor, program_bytes, plan, params } = w.register() else { unreachable!() };
        let pc = misaka_palw_kernel::trace::ParamCommitmentsV1::of(&params);
        O::RegisterClass { descriptor, program_bytes, plan, param_commitments: pc }
    };
    let lid_job = job2.id();
    let samples = vec![
        (
            "FileProof/Kernel vs lying",
            O::FileProof { accuser: OUTSIDER, claim: lid, proof: ProsecutionV1::Kernel(proof.clone()) },
            OUTSIDER,
        ),
        ("FileProof/Kernel vs honest", O::FileProof { accuser: OUTSIDER, claim: hid, proof: ProsecutionV1::Kernel(proof) }, OUTSIDER),
        (
            "FileProof/Decode vs honest",
            O::FileProof { accuser: OUTSIDER, claim: hid, proof: ProsecutionV1::Decode(decode.clone()) },
            OUTSIDER,
        ),
        ("FileProof/Decode vs lying", O::FileProof { accuser: OUTSIDER, claim: lid, proof: ProsecutionV1::Decode(decode) }, OUTSIDER),
        ("Respond", O::Respond { claim: lid, stage: 0, position: 1, bytes: resp }, SPAM1),
        ("FileDemand", O::FileDemand { demander: SPAM1, claim: lid, stage: 0, position: 2 }, SPAM1),
        ("CommitClaim", O::CommitClaim { claim, evidence, commitments }, PRODUCER),
        ("RegisterClass", register, PRODUCER),
        ("PostJob", O::PostJob { job: KernelJobLike::job(&w, 77) }, PRODUCER),
        ("RequestExit", O::RequestExit { bond: SPAM1 }, SPAM1),
        ("Withdraw", O::Withdraw { bond: SPAM1 }, SPAM1),
        ("SealClaim", O::SealClaim { producer: SPAM1, job: lid_job, seal: [0x5E; 64] }, SPAM1),
    ];
    Fixture { ledger: w.l.clone(), book: Book::from_ledger(&w.l), honest: hid, lying: lid, samples }
}

/// A job-like helper (the `World` posts jobs through blocks; the fuzz fixture needs one more for a fresh valid `CommitClaim`).
struct KernelJobLike;
impl KernelJobLike {
    fn job(w: &World, nonce: u8) -> misaka_palw_kernel::job::KernelJobV1 {
        misaka_palw_kernel::job::KernelJobV1 {
            class_binding_id: w.class,
            prompt: vec![3, 17, 9],
            max_new_tokens: 3,
            decode: misaka_palw_kernel::job::DecodeRuleV1::Greedy,
            nonce: [nonce; 64],
        }
    }
}

fn extreme_shapes() -> Vec<Vec<u64>> {
    vec![
        vec![u64::MAX, 2],
        vec![1 << 32, 1 << 32],
        vec![u64::MAX],
        vec![0, u64::MAX],
        vec![1 << 63, 2],
        vec![3, 0],
        vec![],
        vec![u64::MAX, u64::MAX, u64::MAX],
        vec![1 << 40, 1 << 40, 1],
        vec![1; 300],
        vec![0, 1 << 40, 1 << 40],
        vec![0, u64::MAX, u64::MAX],
        vec![1 << 40, 0, 1 << 40],
        vec![u64::MAX, 0],
    ]
}

fn tensor_mutants(t: &TensorWireV1) -> Vec<TensorWireV1> {
    let mut v = Vec::new();
    for s in extreme_shapes() {
        v.push(TensorWireV1 { shape: s, ..t.clone() });
    }
    for d in [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 12, 255] {
        v.push(TensorWireV1 { dtype: d, ..t.clone() });
    }
    v.push(TensorWireV1 { bytes: vec![], ..t.clone() });
    v.push(TensorWireV1 { bytes: t.bytes[..t.bytes.len().saturating_sub(1)].to_vec(), ..t.clone() });
    v
}

fn opening_mutants(o: &TensorOpeningV1) -> Vec<TensorOpeningV1> {
    let mut v = Vec::new();
    for s in extreme_shapes() {
        v.push(TensorOpeningV1 { shape: s, ..o.clone() });
    }
    for a in [2u8, 255] {
        v.push(TensorOpeningV1 { axis: a, ..o.clone() });
    }
    for i in [u64::MAX, 1 << 63, o.index + 1] {
        v.push(TensorOpeningV1 { index: i, ..o.clone() });
    }
    v.push(TensorOpeningV1 { values: vec![i128::MAX; 4], ..o.clone() });
    v.push(TensorOpeningV1 { values: vec![i128::MIN], ..o.clone() });
    v.push(TensorOpeningV1 { values: vec![], ..o.clone() });
    v.push(TensorOpeningV1 { siblings: vec![[7; 64]; 200], ..o.clone() });
    v.push(TensorOpeningV1 { siblings: vec![], ..o.clone() });
    v.push(TensorOpeningV1 { dtype: 255, ..o.clone() });
    v
}

/// Structured mutants of one sample: the same object with one embedded tensor / opening / scalar replaced by an extreme one.
fn structured(name: &str, obj: &O) -> Vec<(String, O)> {
    let mut out: Vec<(String, O)> = Vec::new();
    match obj {
        O::FileProof { accuser, claim, proof: ProsecutionV1::Kernel(bytes) } => {
            let Ok(w) = borsh::from_slice::<FaultProofWireV1>(bytes) else { return out };
            let put =
                |m: FaultProofWireV1| O::FileProof { accuser: *accuser, claim: *claim, proof: ProsecutionV1::Kernel(m.to_bytes()) };
            for (i, t) in tensor_mutants(&w.output).into_iter().enumerate() {
                out.push((format!("{name}/output#{i}"), put(FaultProofWireV1 { output: t, ..w.clone() })));
            }
            for k in 0..w.inputs.len() {
                for (i, t) in tensor_mutants(&w.inputs[k]).into_iter().enumerate() {
                    let mut m = w.clone();
                    m.inputs[k] = t;
                    out.push((format!("{name}/input{k}#{i}"), put(m)));
                }
            }
            for k in 0..w.prior_rows.len() {
                for (i, t) in tensor_mutants(&w.prior_rows[k]).into_iter().enumerate() {
                    let mut m = w.clone();
                    m.prior_rows[k] = t;
                    out.push((format!("{name}/prior{k}#{i}"), put(m)));
                }
            }
            if let Some(op) = &w.openings {
                for (which, pick) in [("x", 0), ("w", 1), ("y", 2)] {
                    let base = match pick {
                        0 => &op.x,
                        1 => &op.w,
                        _ => &op.y,
                    };
                    for (i, m) in opening_mutants(base).into_iter().enumerate() {
                        let mut ops = op.clone();
                        match pick {
                            0 => ops.x = m,
                            1 => ops.w = m,
                            _ => ops.y = m,
                        }
                        out.push((format!("{name}/open-{which}#{i}"), put(FaultProofWireV1 { openings: Some(ops), ..w.clone() })));
                    }
                }
            }
            for (i, (p, oc, n)) in [(u32::MAX, 0, 0), (0, u16::MAX, 0), (0, 0, u16::MAX), (1, 0, 0), (u32::MAX, u16::MAX, u16::MAX)]
                .into_iter()
                .enumerate()
            {
                out.push((format!("{name}/where#{i}"), put(FaultProofWireV1 { position: p, occurrence: oc, node: n, ..w.clone() })));
            }
            for k in [0u8, 1, 2, 3, 4, 255] {
                out.push((format!("{name}/kind{k}"), put(FaultProofWireV1 { kind: k, ..w.clone() })));
            }
            for sc in [
                (u64::MAX, u64::MAX, u64::MAX),
                (0, 0, 0),
                (u64::MAX, 0, 0),
                (0, u64::MAX, 0),
                (0, 0, u64::MAX),
                (1 << 63, 1 << 63, 1 << 63),
            ] {
                out.push((format!("{name}/scalar{sc:?}"), put(FaultProofWireV1 { scalar: sc, ..w.clone() })));
            }
        }
        O::FileProof { accuser, claim, proof: ProsecutionV1::Decode(f) } => {
            for (i, t) in tensor_mutants(&f.logits).into_iter().enumerate() {
                out.push((
                    format!("{name}/logits#{i}"),
                    O::FileProof {
                        accuser: *accuser,
                        claim: *claim,
                        proof: ProsecutionV1::Decode(DecodeFaultV1 { index: f.index, logits: t }),
                    },
                ));
            }
            for idx in [u32::MAX, 1 << 31, 2, 3] {
                out.push((
                    format!("{name}/index{idx}"),
                    O::FileProof {
                        accuser: *accuser,
                        claim: *claim,
                        proof: ProsecutionV1::Decode(DecodeFaultV1 { index: idx, logits: f.logits.clone() }),
                    },
                ));
            }
        }
        O::Respond { claim, stage, position, bytes } => {
            let Ok(r) = borsh::from_slice::<PositionResponseV1>(bytes) else { return out };
            let put = |m: PositionResponseV1| O::Respond {
                claim: *claim,
                stage: *stage,
                position: *position,
                bytes: borsh::to_vec(&m).unwrap(),
            };
            for (s, occ) in r.values.iter().enumerate() {
                for (n, v) in occ.iter().enumerate() {
                    if let MaterialResponseV1::Whole(t) = v {
                        for (i, m) in tensor_mutants(t).into_iter().enumerate() {
                            let mut rr = r.clone();
                            rr.values[s][n] = MaterialResponseV1::Whole(m);
                            out.push((format!("{name}/val{s}.{n}#{i}"), put(rr)));
                        }
                        // The same value as a (forged) partial opening of an extreme shape.
                        let fake = TensorOpeningV1 {
                            dtype: t.dtype,
                            shape: t.shape.clone(),
                            axis: 0,
                            index: 0,
                            values: vec![0; 1],
                            siblings: vec![],
                            other_root: [0; 64],
                        };
                        for (i, m) in opening_mutants(&fake).into_iter().enumerate() {
                            let mut rr = r.clone();
                            rr.values[s][n] = MaterialResponseV1::Part(m);
                            out.push((format!("{name}/part{s}.{n}#{i}"), put(rr)));
                        }
                        break; // one tensor per occurrence is enough to cover the path; the byte fuzz covers the rest
                    }
                }
            }
        }
        _ => {}
    }
    out
}

/// Byte-level mutants of the encoding: flips over a spread of offsets, truncations, a trailing byte.
fn byte_mutants(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let n = bytes.len();
    // Small objects are flipped at every offset; big ones at the head and a spread through the body.
    let mut offs: Vec<usize> = (0..n.min(if n < 2000 { n } else { 160 })).collect();
    if n >= 2000 {
        offs.extend((160..n).step_by((n / 150).max(1)));
    }
    offs.sort();
    offs.dedup();
    let mut v = Vec::new();
    for i in offs {
        for (tag, f) in [("xor80", 0x80u8), ("xorff", 0xFF)] {
            let mut b = bytes.to_vec();
            b[i] ^= f;
            v.push((format!("byte{i}/{tag}"), b));
        }
    }
    for cut in [0, 1, 2, 3, 10, 66, 130, n / 2, n.saturating_sub(65), n.saturating_sub(5), n.saturating_sub(1)] {
        if cut < n {
            v.push((format!("trunc{cut}"), bytes[..cut].to_vec()));
        }
    }
    let mut t = bytes.to_vec();
    t.push(0);
    v.push(("trailing".into(), t));
    v
}

fn run(f: &Fixture, label: &str, signer: Digest, bytes: &[u8], stats: &mut BTreeMap<&'static str, u32>) {
    let mut l = f.ledger.clone();
    let mut bk = f.book.clone();
    l.begin_block(f.ledger.daa + 1).unwrap();
    let before = l.root();
    let r = catch_unwind(AssertUnwindSafe(|| l.apply_encoded(bytes, &AuthV1 { signer_bond: signer })));
    match r {
        Err(_) => {
            *stats.entry("panic").or_default() += 1;
            let mut p = PANICS.lock().unwrap();
            let last = p.last_mut().expect("hook recorded it");
            last.1 = format!("{} :: {label}", last.1);
        }
        Ok(Err(_)) => {
            *stats.entry("refused").or_default() += 1;
            assert_eq!(l.root(), before, "I2 violated: a refusal changed the state ({label})");
        }
        Ok(Ok(ev)) => {
            *stats.entry("applied").or_default() += 1;
            let honest_convicted = ev.iter().any(|e| matches!(e, E::Convicted { claim, .. } if *claim == f.honest));
            assert!(!honest_convicted, "I3 violated: an honest claim was convicted by a mutant ({label})");
            let _ = f.lying;
            // I4: every settlement instruction an applied mutant emits is applicable to a real bond book, slashes are routed 1:1,
            // value is conserved and the ledger's view of every bond is the book's.
            for e in &ev {
                if let E::Settlement(s) = e {
                    bk.apply(s).unwrap_or_else(|why| panic!("I4 violated: a settlement a real bond refuses ({label}): {s:?}: {why}"));
                }
            }
            bk.check(&l).unwrap_or_else(|why| panic!("I4 violated: accounting ({label}): {why}"));
        }
    }
}

#[test]
fn a_stranger_who_mutates_a_valid_object_cannot_panic_the_fold_or_convict_an_honest_claim() {
    install_hook();
    let f = fixture();
    let mut stats: BTreeMap<&'static str, u32> = BTreeMap::new();
    for (name, obj, signer) in &f.samples {
        let before = stats.clone();
        let enc = obj.encode();
        // The unmutated sample is a legitimate object: it must decode and (for the honest-claim target) never convict.
        assert_eq!(&O::decode(&enc).expect("canonical"), obj, "{name}");
        run(&f, &format!("{name}/original"), *signer, &enc, &mut stats);
        for (m, o) in structured(name, obj) {
            run(&f, &m, *signer, &o.encode(), &mut stats);
        }
        for (m, b) in byte_mutants(&enc) {
            run(&f, &format!("{name}/{m}"), *signer, &b, &mut stats);
        }
        let d = |k: &'static str| stats.get(k).copied().unwrap_or(0) - before.get(k).copied().unwrap_or(0);
        eprintln!("  {name}: {} bytes, applied {}, refused {}, panicked {}", enc.len(), d("applied"), d("refused"), d("panic"));
    }
    let _ = std::panic::take_hook();
    let panics = PANICS.lock().unwrap().clone();
    let mut by_loc: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for (loc, msg) in &panics {
        let e = by_loc.entry(loc.clone()).or_insert((0, String::new()));
        e.0 += 1;
        if e.1.len() < 300 {
            e.1.push_str(&format!("[{msg}] "));
        }
    }
    eprintln!("mutants: {stats:?}");
    for (loc, (n, first)) in &by_loc {
        eprintln!("PANIC x{n} at {loc}: {first}");
    }
    assert!(by_loc.is_empty(), "I1 violated: {} distinct panic sites reachable from encoded route objects", by_loc.len());
}

/// EXPECTED (before running): the registration payload is the biggest attacker-chosen parse surface (program bytes, plan, artifact
/// commitments, decoded by the TIR decoder and analysed by the plan checker). Flipping EVERY byte of a valid `RegisterClass` three
/// ways never panics the fold, and a refusal leaves the root unchanged. VERDICT: PASS expected.
#[test]
fn every_byte_flip_of_a_class_registration_is_refused_or_applied_without_a_panic() {
    install_hook();
    let w = World::new();
    let T::RegisterClass { descriptor, program_bytes, plan, params } = w.register() else { unreachable!() };
    let pc = misaka_palw_kernel::trace::ParamCommitmentsV1::of(&params);
    // A minimal ledger: the genesis configuration, one zero-collateral bond, the artifact attested; no class yet.
    let mut base = w.genesis.clone();
    base.begin_block(1).unwrap();
    base.sync_bond(PRODUCER, 0);
    base.attest_artifact(pc.root());
    let obj = O::RegisterClass { descriptor, program_bytes, plan, param_commitments: pc };
    {
        let mut ok = base.clone();
        ok.begin_block(2).unwrap();
        ok.apply_encoded(&obj.encode(), &AuthV1 { signer_bond: PRODUCER })
            .expect("the unflipped registration applies to the minimal ledger");
    }
    let (applied, refused, panics) = support::exhaustive_flips(&base, &obj, PRODUCER);
    let _ = std::panic::take_hook();
    eprintln!("RegisterClass byte flips: {} bytes, applied {applied}, refused {refused}, panics {}", obj.encoded_len(), panics.len());
    for (at, what) in panics.iter().take(10) {
        eprintln!("  PANIC at byte {at} ({what})");
    }
    for (loc, msg) in PANICS.lock().unwrap().iter().take(10) {
        eprintln!("  site {loc}: {msg}");
    }
    assert!(panics.is_empty(), "I1 violated: {} flips of a RegisterClass panic the fold", panics.len());
}
