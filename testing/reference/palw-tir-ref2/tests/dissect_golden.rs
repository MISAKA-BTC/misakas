//! 04b §9.5.8's golden vectors (`consensus-vectors/tir-v1/dissect/`), reproduced by this crate's
//! independent H dissection: every case's site, element closure, honest totals, finalize, first cut
//! at arity 2 and bottom, from the run the file's `job` describes; and the court's checks along the
//! way (the root claim is admitted, every round folds, the bottom finds no fault).

mod common;

use common::demsrc::Model;
use common::*;
use misaka_palw_tir_ref2::admit::ranges;
use misaka_palw_tir_ref2::codec::decode_canonical;
use misaka_palw_tir_ref2::demand::{Ctx, Limits, Target, eval_demanded};
use misaka_palw_tir_ref2::dissect::{
    Fold, RootClaim, Verdict, admit_root, bottom, check_round, closure, cut, finalize, history_tiles, partials, positions, site,
};
use misaka_palw_tir_ref2::eval::Params;
use misaka_palw_tir_ref2::{Program, Tensor};
use serde_json::Value;

fn u64_of(v: &Value) -> u64 {
    int_of(v) as u64
}

fn ints(v: &Value) -> Vec<i128> {
    v.as_array().unwrap().iter().map(int_of).collect()
}

fn lists(v: &Value) -> Vec<Vec<i128>> {
    v.as_array().unwrap().iter().map(ints).collect()
}

fn params_of(prog: &Program, v: &Value) -> Params {
    let mut params = Params::new();
    for e in v.as_array().unwrap() {
        let j = int_of(&e["param"]) as u16;
        let layer = if e["layer"].is_null() { None } else { Some(int_of(&e["layer"]) as u32) };
        let d = &prog.params[j as usize];
        let t = Tensor::from_le_bytes(d.dtype, d.shape.iter().map(|&x| x as u64).collect(), &hex_decode(e["le_hex"].as_str().unwrap()))
            .unwrap();
        params.insert((j, layer), t);
    }
    params
}

#[test]
fn dissect_vectors() {
    let dir = vectors_dir().join("dissect");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    assert!(!files.is_empty());
    let (mut total, mut ok) = (0, 0);
    let mut fails: Vec<String> = Vec::new();
    for f in &files {
        let d = read_json(f);
        assert_eq!(d["format"], "palw-tir-v1/dissect-vectors/1");
        // A vector names its program's file, or carries the program inline (the H-series' own: H1, H2).
        let pv = match d["program"].as_str() {
            Some(path) => read_json(&vectors_dir().join(path)),
            None => d["inline"].clone(),
        };
        let prog = decode_canonical(&hex_decode(pv["program_borsh_hex"].as_str().unwrap())).unwrap();
        let params = params_of(&prog, &pv["params"]);
        let job = &d["job"];
        let h_tile = u64_of(&job["h_tile"]);
        let prompt: Vec<u64> = job["prompt"].as_array().unwrap().iter().map(u64_of).collect();
        let generated: Vec<u64> = job["generated"].as_array().unwrap().iter().map(u64_of).collect();
        assert_eq!(prompt.len() as u64, u64_of(&job["prefill"]));
        assert_eq!(generated.len() as u64, u64_of(&job["decode"]));
        // The run: the prompt, then every generated token but the last fed back.
        let mut tokens = prompt.clone();
        tokens.extend_from_slice(&generated[..generated.len() - 1]);
        let m = Model::from_run(&prog, &params, &tokens, &|_| true).expect("the job's run succeeds");
        let intervals = ranges(&prog).expect("the program's ranges");
        let lim = Limits::UNLIMITED;
        let (mut ft, mut fo) = (0, 0);
        for c in d["cases"].as_array().unwrap() {
            ft += 1;
            let name = format!("{} / {}", d["name"].as_str().unwrap(), c["name"].as_str().unwrap());
            let mut bad: Vec<String> = Vec::new();
            let leaf = &c["leaf"];
            let ctx = Ctx { pos: u64_of(&leaf["pos"]), occ: u64_of(&leaf["occurrence"]) as u32 };
            let n = u64_of(&leaf["node"]) as u16;
            let first = u64_of(&leaf["first_element"]);
            let tile: Vec<u64> = (first..first + u64_of(&leaf["values"])).collect();
            let st = match site(&prog, &intervals, ctx, n, h_tile) {
                Ok(Some(s)) => s,
                other => {
                    fails.push(format!("{name}: site {other:?}"));
                    continue;
                }
            };
            // The site.
            let vs = &c["site"];
            let folds: Vec<&str> = st.folds.iter().map(|f| if *f == Fold::Max { "max" } else { "sum" }).collect();
            let want_folds: Vec<&str> = vs["folds"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
            let bounds: Vec<(i128, i128)> = st.bounds.iter().map(|b| (b.lo, b.hi)).collect();
            let want_bounds: Vec<(i128, i128)> =
                vs["bounds"].as_array().unwrap().iter().map(|b| (int_of(&b["lo"]), int_of(&b["hi"]))).collect();
            if st.reductions.iter().map(|&r| r as i128).collect::<Vec<_>>() != ints(&vs["reductions"])
                || folds != want_folds
                || bounds != want_bounds
                || st.counts.iter().map(|&x| x as i128).collect::<Vec<_>>() != ints(&vs["counts"])
                || st.h != u64_of(&vs["h"])
                || st.h_tile != u64_of(&vs["h_tile"])
            {
                bad.push(format!("site {st:?} vs {vs}"));
            }
            // The honest totals, every element of every reduction, from the run itself (no supply).
            let mut all_t: Vec<Vec<i128>> = Vec::new();
            for (i, &r) in st.reductions.iter().enumerate() {
                let els: Vec<u64> = (0..st.counts[i]).collect();
                let mut src = common::demsrc::Mine(&m);
                match eval_demanded(&prog, &Target::Node { ctx, node: r }, &els, &mut src, lim) {
                    Ok((v, _)) => all_t.push(v),
                    Err(e) => bad.push(format!("total of reduction {r}: {e:?}")),
                }
            }
            if !bad.is_empty() {
                fails.push(format!("{name}: {}", bad.join(" | ")));
                continue;
            }
            let full = RootClaim { elements: st.counts.iter().map(|&k| (0..k).collect()).collect(), totals: all_t.clone() };
            // The element closure, with every total supplied.
            let cl = match closure(&prog, &st, &tile, &full, &mut common::demsrc::Mine(&m), lim) {
                Ok(c) => c,
                Err(e) => {
                    fails.push(format!("{name}: closure {e:?}"));
                    continue;
                }
            };
            let l: Vec<Vec<u64>> = cl.iter().map(|s| s.iter().copied().collect()).collect();
            let want_l: Vec<Vec<u64>> = c["elements"].as_array().unwrap().iter().map(|x| x.as_array().unwrap().iter().map(u64_of).collect()).collect();
            if l != want_l {
                bad.push(format!("elements {l:?} vs {want_l:?}"));
            }
            let totals: Vec<Vec<i128>> = l.iter().enumerate().map(|(i, li)| li.iter().map(|&e| all_t[i][e as usize]).collect()).collect();
            if totals != lists(&c["totals"]) {
                bad.push(format!("totals {totals:?} vs {}", c["totals"]));
            }
            let root = RootClaim { elements: l, totals };
            // The finalize reproduces the committed tile.
            let committed: Vec<i128> =
                tile.iter().map(|&e| m.nodes[&(ctx.pos, ctx.occ, n)][e as usize]).collect();
            match finalize(&prog, &st, &tile, &root, &mut common::demsrc::Mine(&m), lim) {
                Ok((v, _)) => {
                    if v != ints(&c["finalize"]) || v != committed {
                        bad.push(format!("finalize {v:?} vs {} (committed {committed:?})", c["finalize"]));
                    }
                }
                Err(e) => bad.push(format!("finalize {e:?}")),
            }
            if let Err(r) = admit_root(&prog, &st, &tile, &committed, &root, &mut common::demsrc::Mine(&m), lim) {
                bad.push(format!("the honest root claim is refused: {r:?}"));
            }
            // The first cut at arity 2, and the bottom reached by naming the last child each round.
            let t_h = history_tiles(&st);
            let (mut first_t, mut count, mut claim) = (0u64, t_h, root.totals.clone());
            let mut round = 0;
            while count > 1 {
                let children = cut(first_t, count, 2);
                let mut claims = Vec::new();
                for &(cf, cc) in &children {
                    let range = positions(&st, cf, cc);
                    match partials(&prog, &st, &root, range, &mut common::demsrc::Mine(&m), lim) {
                        Ok(pv) => claims.push(pv),
                        Err(e) => bad.push(format!("partials over {range:?}: {e:?}")),
                    }
                }
                if claims.len() != children.len() {
                    break;
                }
                if let Err(r) = check_round(&st, &root, &claim, &claims, children.len()) {
                    bad.push(format!("round {round} does not fold: {r:?}"));
                }
                if round == 0 {
                    let vc = c["cut"].as_array().unwrap();
                    if vc.len() != children.len() {
                        bad.push(format!("{} children vs {}", children.len(), vc.len()));
                    }
                    for (k, (x, &(cf, cc))) in vc.iter().zip(children.iter()).enumerate() {
                        let (pf, pt) = positions(&st, cf, cc);
                        if ints(&x["tiles"]) != vec![cf as i128, cc as i128]
                            || ints(&x["positions"]) != vec![pf as i128, pt as i128]
                            || lists(&x["partials"]) != claims[k]
                        {
                            bad.push(format!("cut child {k}: ours tiles ({cf}, {cc}) positions ({pf}, {pt}) partials {:?} vs {x}", claims[k]));
                        }
                    }
                }
                let last = children.len() - 1;
                (first_t, count) = children[last];
                claim = claims[last].clone();
                round += 1;
            }
            let range = positions(&st, first_t, count);
            let vb = &c["bottom"];
            if ints(&vb["positions"]) != vec![range.0 as i128, range.1 as i128] {
                bad.push(format!("bottom positions {range:?} vs {}", vb["positions"]));
            }
            match partials(&prog, &st, &root, range, &mut common::demsrc::Mine(&m), lim) {
                Ok(pv) if pv == lists(&vb["partials"]) => {}
                Ok(pv) => bad.push(format!("bottom partials {pv:?} vs {}", vb["partials"])),
                Err(e) => bad.push(format!("bottom {e:?}")),
            }
            match bottom(&prog, &st, &root, range, &claim, &mut common::demsrc::Mine(&m), lim) {
                Ok(Verdict::Defeated) => {}
                other => bad.push(format!("the honest bottom: {other:?}")),
            }
            if bad.is_empty() {
                fo += 1;
            } else {
                fails.push(format!("{name}: {}", bad.join(" | ")));
            }
        }
        println!("{}: {fo}/{ft}", f.file_name().unwrap().to_string_lossy());
        total += ft;
        ok += fo;
    }
    println!("dissect vectors: {ok}/{total} reproduced");
    for f in &fails {
        println!("  FAIL {f}");
    }
    assert!(fails.is_empty(), "{} dissect vectors not reproduced", fails.len());
}
