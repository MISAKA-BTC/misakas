//! **Spec 17 §17.13: the Model Improvement Protocol's golden vectors** (`consensus-vectors/improve-v1/`).
//!
//! Regenerated from the pure functions every implementation must agree on — the pinned sign table,
//! the policy digest and message, the material tree, the epoch seed and the draw, the suite draw and items,
//! the judge draw, a suite dataset's RFC 6962 roots and proofs, a judge template's fills and a judged score's
//! parts, and the promotion rule over score tables — and compared byte for byte with the files. `IMPROVE_BLESS=1` rewrites them, which is a change of the semantics and is
//! reviewed as one. (`transitions.json` and `pool.json` come from the fold's own suite.)

use std::collections::BTreeMap;
use std::path::PathBuf;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_improve_epoch_v1::{
    PalwDrawEntryV1, PalwMaterialKindV1, palw_improve_dataset_root_v1, palw_improve_draw_key_v1, palw_improve_draw_v1,
    palw_improve_epoch_seed_v1, palw_improve_judge_index_v1, palw_improve_material_append_v1, palw_improve_material_leaf_v1,
    palw_improve_material_root_v1, palw_improve_setter_item_id_v1, palw_improve_suite_draw_v1, palw_improve_suite_item_id_v1,
};
use kaspa_consensus_core::palw_improve_eval_v1::{
    PalwEvalStageParamsV1, PalwJudgeTemplateV1, PalwSuiteEntryV1, PalwSuiteReferenceV1, palw_improve_judge_fill_v1,
    palw_improve_judge_template_root_v1, palw_improve_judged_score_v1, palw_improve_suite_leaf_v1, palw_improve_suite_node_v1,
    palw_improve_suite_proof_v1, palw_improve_suite_root_v1, palw_improve_suite_verify_v1,
};
use kaspa_consensus_core::palw_improve_policy_v1::{
    palw_improvement_policy_check_v1, palw_improvement_policy_example_v1, palw_improvement_policy_message_v1,
};
use kaspa_consensus_core::palw_improve_promotion_v1::{
    PALW_IMPROVE_SIGN_ALPHAS_PERMILLE_V1, PALW_IMPROVE_SIGN_TABLE_ID_HEX_V1, PalwImproveRuleV1, PalwImproveScoresV1,
    palw_improve_counts_v1, palw_improve_decide_v1, palw_improve_eligible_v1, palw_improve_sign_critical_v1,
};
use kaspa_consensus_core::palw_improve_state_v1::{
    PalwEvalItemV1, PalwEvalSubjectV1, PalwImprovementPolicyV1, PalwItemSourceV1, PalwJudgeSpecV1, PalwMaterialFrontierV1,
    PalwScoringKindV1, PalwScoringParamsV1, PalwScoringStageV1, palw_improvement_policy_digest_v1,
};
use kaspa_consensus_core::palw_improve_v1::PALW_DRILL_IMPROVE_CEILINGS_V1;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::tx::TransactionOutpoint;
use serde_json::{Value, json};

fn h(byte: u8) -> Hash64 {
    Hash64::from_bytes([byte; 64])
}

fn bond(byte: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: h(byte), index: 0 })
}

fn hex(h: &Hash64) -> String {
    h.to_string()
}

fn sign_table() -> Value {
    let mut samples = Vec::new();
    for alpha in PALW_IMPROVE_SIGN_ALPHAS_PERMILLE_V1 {
        for k in [1u32, 2, 4, 64] {
            for m in (0..=20u32).chain([100, 400, 2047, 2048]) {
                samples.push(
                    json!({ "alpha_permille": alpha, "K": k, "m": m, "k": palw_improve_sign_critical_v1(m, alpha, k).unwrap() }),
                );
            }
        }
    }
    json!({
        "definition": "k(m, α/K) = the smallest k in 0..=m+1 with 1000·K·Σ_{i=k}^{m} C(m,i) ≤ α_permille·2^m",
        "byte_form": "b\"PALW-IMPROVE-SIGN-TABLE-V1\" ‖ LE u16 2048 ‖ u8 64 ‖ u8 2 ‖ LE u16 10 ‖ LE u16 50 ‖ LE u32 k for α, then K ascending, then m ascending",
        "byte_len": 26 + 2 + 1 + 1 + 4 + 4 * 2 * 64 * 2049,
        "sign_table_id": PALW_IMPROVE_SIGN_TABLE_ID_HEX_V1,
        "samples": samples,
    })
}

/// A judged stage's specification: "Yes"/"No" verdicts over a template dataset.
fn judge_spec(dataset: u8) -> PalwJudgeSpecV1 {
    PalwJudgeSpecV1 { template_dataset: h(dataset), verdict_a: vec![7], verdict_b: vec![2, 9], logit_scale_q24: 4096 }
}

/// The example with a Judge stage and its specification.
fn judged(p: &mut PalwImprovementPolicyV1) {
    p.eval
        .stages
        .push(PalwScoringStageV1 { kind: PalwScoringKindV1::Judge, params: PalwScoringParamsV1::Judge { lo: -1_000, hi: 1_000 } });
    p.eval.judge_set = vec![h(0xD1)];
    p.eval.judge = Some(judge_spec(0x71));
}

/// The example's windows lengthened to hold a 4,820-DAA claim lifecycle.
fn long_windows(p: &mut PalwImprovementPolicyV1) {
    p.windows.court_margin = 4_820;
    p.windows.grid = 6_000;
}

fn policy() -> Value {
    let example = palw_improvement_policy_example_v1();
    let domain = h(0xA1);
    let line = h(0xA2);
    // `lifecycle` is the ruleset's claim lifecycle bound (spec 17 §17.4.3 rows 19–20): `None` where a fixture sets none.
    type Edit = Box<dyn Fn(&mut PalwImprovementPolicyV1)>;
    let edits: Vec<(&str, Option<u64>, Edit)> = vec![
        ("the example", None, Box::new(|_| {})),
        ("w_eval = beacon_delay + 32 (E12)", None, Box::new(|p| p.windows.w_eval = p.windows.beacon_delay + 32)),
        ("w_eval = beacon_delay + 33", None, Box::new(|p| p.windows.w_eval = p.windows.beacon_delay + 33)),
        ("grid below L_e", None, Box::new(|p| p.windows.grid = 900)),
        ("α = 25 (untabled)", None, Box::new(|p| p.eval.alpha_permille = 25)),
        ("α = 10", None, Box::new(|p| p.eval.alpha_permille = 10)),
        ("k_max = 9 (past the drill's 8)", None, Box::new(|p| p.k_max = 9)),
        ("n_min > n", None, Box::new(|p| p.eval.n_min = p.eval.n + 1)),
        ("no primary stage", None, Box::new(|p| p.eval.stages.clear())),
        ("a suite with items naming no dataset", None, Box::new(|p| p.eval.regression_dataset = Hash64::default())),
        ("a suite without items naming a dataset", None, Box::new(|p| p.eval.safety_items = 0)),
        ("seat_pool_permille at the drill's ceiling (500)", None, Box::new(|p| p.eval.seat_pool_permille = 500)),
        ("seat_pool_permille past the ceiling (501)", None, Box::new(|p| p.eval.seat_pool_permille = 501)),
        ("a Judge stage with its specification", None, Box::new(judged)),
        (
            "a Judge stage without a specification",
            None,
            Box::new(|p| {
                judged(p);
                p.eval.judge = None;
            }),
        ),
        (
            "a judge specification without a Judge stage",
            None,
            Box::new(|p| {
                judged(p);
                p.eval.stages.pop();
                p.eval.judge_set.clear();
            }),
        ),
        (
            "equal verdict sequences",
            None,
            Box::new(|p| {
                judged(p);
                p.eval.judge.as_mut().unwrap().verdict_b = vec![7];
            }),
        ),
        (
            "a verdict of nine ids",
            None,
            Box::new(|p| {
                judged(p);
                p.eval.judge.as_mut().unwrap().verdict_a = vec![1; 9];
            }),
        ),
        (
            "an empty verdict",
            None,
            Box::new(|p| {
                judged(p);
                p.eval.judge.as_mut().unwrap().verdict_a = vec![];
            }),
        ),
        (
            "a zero logit scale",
            None,
            Box::new(|p| {
                judged(p);
                p.eval.judge.as_mut().unwrap().logit_scale_q24 = 0;
            }),
        ),
        (
            "no template dataset",
            None,
            Box::new(|p| {
                judged(p);
                p.eval.judge.as_mut().unwrap().template_dataset = Hash64::default();
            }),
        ),
        ("court_margin 150 under a 4,820-DAA claim lifecycle", Some(4_820), Box::new(|_| {})),
        ("court_margin 4,820 under a 4,820-DAA claim lifecycle", Some(4_820), Box::new(long_windows)),
        (
            "court_margin 4,819 under a 4,820-DAA claim lifecycle",
            Some(4_820),
            Box::new(|p| {
                long_windows(p);
                p.windows.court_margin = 4_819;
            }),
        ),
        (
            "a judged policy with w_eval 300 under a 4,820-DAA claim lifecycle",
            Some(4_820),
            Box::new(|p| {
                long_windows(p);
                judged(p);
            }),
        ),
        (
            "a judged policy with w_eval = beacon_delay + 4,820 + 32",
            Some(4_820),
            Box::new(|p| {
                long_windows(p);
                judged(p);
                p.windows.w_eval = p.windows.beacon_delay + 4_820 + 32;
                p.windows.grid = 12_000;
            }),
        ),
        (
            "a judged policy with w_eval = beacon_delay + 4,820 + 33",
            Some(4_820),
            Box::new(|p| {
                long_windows(p);
                judged(p);
                p.windows.w_eval = p.windows.beacon_delay + 4_820 + 33;
                p.windows.grid = 12_000;
            }),
        ),
    ];
    let checks: Vec<Value> = edits
        .into_iter()
        .map(|(name, lifecycle, edit)| {
            let mut p = example.clone();
            edit(&mut p);
            let verdict = palw_improvement_policy_check_v1(&p, &PALW_DRILL_IMPROVE_CEILINGS_V1, lifecycle);
            json!({ "case": name, "lifecycle": lifecycle, "accepted": verdict.is_ok() })
        })
        .collect();
    json!({
        "ceilings": "PALW_DRILL_IMPROVE_CEILINGS_V1",
        "example_borsh_len": borsh::to_vec(&example).unwrap().len(),
        "example_digest": hex(&palw_improvement_policy_digest_v1(&example)),
        "message": {
            "network_domain": hex(&domain),
            "line_id": hex(&line),
            "set_seq_1": hex(&palw_improvement_policy_message_v1(domain, &line, 1, Some(&example))),
            "opt_out_seq_2": hex(&palw_improvement_policy_message_v1(domain, &line, 2, None)),
        },
        "checks": checks,
    })
}

fn material() -> Value {
    let mut frontier = PalwMaterialFrontierV1::default();
    let mut rows = vec![
        json!({ "leaves": 0, "root": hex(&palw_improve_material_root_v1(&frontier)), "dataset_root": hex(&palw_improve_dataset_root_v1(&frontier)) }),
    ];
    for i in 0..33u8 {
        let kind = match i % 3 {
            0 => PalwMaterialKindV1::HardCase,
            1 => PalwMaterialKindV1::Dataset,
            _ => PalwMaterialKindV1::TeachingArtifact,
        };
        palw_improve_material_append_v1(&mut frontier, palw_improve_material_leaf_v1(kind, &h(i)));
        let n = frontier.count;
        if [1, 2, 3, 7, 8, 33].contains(&n) {
            rows.push(json!({
                "leaves": n,
                "frontier": frontier.frontier.iter().map(hex).collect::<Vec<_>>(),
                "root": hex(&palw_improve_material_root_v1(&frontier)),
                "dataset_root": hex(&palw_improve_dataset_root_v1(&frontier)),
            }));
        }
    }
    json!({ "leaf_i": "H(material-leaf, kind(i mod 3 → case, dataset, artifact) ‖ [i; 64])", "rows": rows })
}

fn draw() -> Value {
    let block = h(0xB1);
    let line = h(0xB2);
    let seed = palw_improve_epoch_seed_v1(&block, &line, 3);
    let mut pool: Vec<PalwDrawEntryV1> = (0..12u8).map(|i| PalwDrawEntryV1 { id: h(0x10 + i), supplier: bond(i % 3) }).collect();
    let set = h(0xC0);
    pool.extend((0..4u32).map(|i| PalwDrawEntryV1 { id: palw_improve_setter_item_id_v1(&set, i), supplier: bond(9) }));
    let drawn = palw_improve_draw_v1(&seed, &pool, 8, 250);
    let judges = [h(0xD1), h(0xD2), h(0xD3)];
    json!({
        "block": hex(&block), "line": hex(&line), "epoch": 3,
        "seed": hex(&seed),
        "pool": pool.iter().map(|e| json!({ "id": hex(&e.id), "order_key": hex(&palw_improve_draw_key_v1(&seed, &e.id)) })).collect::<Vec<_>>(),
        "n": 8, "setter_cap_permille": 250,
        "drawn_indices": drawn,
        "suite_item_id_of_dataset_E1_entry_5": hex(&palw_improve_suite_item_id_v1(&h(0xE1), 5)),
        "suite_draw": {
            "dataset": hex(&h(0xE1)),
            "regression_5_of_40": palw_improve_suite_draw_v1(&seed, &h(0xE1), 1, 40, 5),
            "safety_5_of_40": palw_improve_suite_draw_v1(&seed, &h(0xE1), 2, 40, 5),
            "all_of_a_dataset_of_4": palw_improve_suite_draw_v1(&seed, &h(0xE1), 1, 4, 9),
            "first_3_of_a_dataset_of_2_pow_32_minus_1": palw_improve_suite_draw_v1(&seed, &h(0xE1), 1, u32::MAX, 3),
            "none_of_an_empty_dataset": palw_improve_suite_draw_v1(&seed, &h(0xE1), 1, 0, 5),
        },
        "judge_of_item": (0..8u32).map(|i| palw_improve_judge_index_v1(&seed, i, judges.len()).unwrap()).collect::<Vec<_>>(),
    })
}

/// A suite dataset of `n` entries (spec 17 §17.6.3): the leaves, the root, and every entry's inclusion proof.
fn suite() -> Value {
    let entry = |i: u32| PalwSuiteEntryV1 {
        prompt: vec![100 + i, 7],
        reference: if i % 2 == 0 {
            PalwSuiteReferenceV1::ExactKey(vec![i, i + 1])
        } else {
            PalwSuiteReferenceV1::Continuation(vec![i])
        },
    };
    let all: Vec<_> = (0..9u32).map(entry).collect();
    let leaves: Vec<_> = all.iter().map(palw_improve_suite_leaf_v1).collect();
    let trees: Vec<Value> = [1usize, 2, 3, 4, 5, 6, 7, 8, 9]
        .iter()
        .map(|n| {
            let l = &leaves[..*n];
            let root = palw_improve_suite_root_v1(l).unwrap();
            let proofs: Vec<Value> = (0..*n)
                .map(|i| {
                    let proof = palw_improve_suite_proof_v1(l, i).unwrap();
                    assert!(palw_improve_suite_verify_v1(&l[i], i as u64, *n as u64, &proof, &root));
                    json!({ "index": i, "proof": proof.iter().map(hex).collect::<Vec<_>>() })
                })
                .collect();
            json!({ "entries": n, "root": hex(&root), "proofs": proofs })
        })
        .collect();
    json!({
        "entry_i": "PalwSuiteEntryV1 { prompt: [100 + i, 7], reference: i even → ExactKey([i, i + 1]), i odd → Continuation([i]) }",
        "leaf": "H(\"misaka-palw/improve/suite-leaf/v1\", borsh(entry))",
        "node": "H(\"misaka-palw/improve/suite-node/v1\", left ‖ right)",
        "tree": "RFC 6962: one leaf is its own root; otherwise the split is at the largest power of two below the count",
        "leaves": leaves.iter().map(hex).collect::<Vec<_>>(),
        "node_of_leaves_0_1": hex(&palw_improve_suite_node_v1(&leaves[0], &leaves[1])),
        "trees": trees,
    })
}

/// A judge template's root and fills, and a judged score's parts (spec 17 §17.8.5).
fn judge() -> Value {
    let judge_template = PalwJudgeTemplateV1 { segments: vec![vec![1, 1], vec![2], vec![3]] };
    let pair_template = PalwJudgeTemplateV1 { segments: vec![vec![1], vec![2], vec![3], vec![4]] };
    let (item, a, b) = (vec![10u32, 11], vec![20u32, 21, 22], vec![30u32]);
    let filled = |t: &PalwJudgeTemplateV1, outputs: &[&[u32]]| palw_improve_judge_fill_v1(t, &item, outputs).unwrap();
    let judge_params = PalwEvalStageParamsV1::Judge { lo: -100, hi: 100, logit_scale_q24: 4096 };
    let pair_params = |margin| PalwEvalStageParamsV1::Pairwise { margin, logit_scale_q24: 4096 };
    let cases: [([i64; 4], i32); 7] = [
        ([-10, -30, -30, -10], 0),
        ([-30, -10, -10, -30], 0),
        ([-10, -30, -10, -30], 0),
        ([-30, -10, -30, -10], 0),
        ([-10, -30, -30, -10], 40),
        ([-10, -30, -30, -10], 39),
        ([0, 0, 0, 0], 0),
    ];
    let pairwise_cases: Vec<Value> = cases
        .iter()
        .map(|(parts, margin)| json!({ "parts": parts, "margin": margin, "outcome": palw_improve_judged_score_v1(&pair_params(*margin), parts) }))
        .collect();
    json!({
        "templates": {
            "judge": { "segments": judge_template.segments, "root": hex(&palw_improve_judge_template_root_v1(&judge_template)) },
            "pairwise": { "segments": pair_template.segments, "root": hex(&palw_improve_judge_template_root_v1(&pair_template)) },
            "root": "H(\"misaka-palw/improve/judge-template/v1\", borsh(template))",
        },
        "fills": {
            "item": item, "output_a": a, "output_b": b,
            "judge_a": filled(&judge_template, &[&a]),
            "pairwise_a_then_b": filled(&pair_template, &[&a, &b]),
            "pairwise_b_then_a": filled(&pair_template, &[&b, &a]),
        },
        "judge_scores": {
            "stage": "Judge { lo: -100, hi: 100 }",
            "parts_to_score": [
                { "ll_a_ll_b": [-30, -50], "score": palw_improve_judged_score_v1(&judge_params, &[-30, -50]) },
                { "ll_a_ll_b": [-50, -30], "score": palw_improve_judged_score_v1(&judge_params, &[-50, -30]) },
                { "ll_a_ll_b": [0, -1_000_000], "score": palw_improve_judged_score_v1(&judge_params, &[0, -1_000_000]) },
                { "ll_a_ll_b": [-1_000_000, 0], "score": palw_improve_judged_score_v1(&judge_params, &[-1_000_000, 0]) },
            ],
        },
        "pairwise_outcomes": {
            "stage": "Pairwise { margin }; parts = [LL(o0,a), LL(o0,b), LL(o1,a), LL(o1,b)]; score = (a0 − b0) − (a1 − b1); +1 iff score > margin else −1",
            "cases": pairwise_cases,
        },
    })
}

#[derive(Default)]
struct Table(BTreeMap<(u32, PalwEvalSubjectV1, u8), i64>);

impl PalwImproveScoresV1 for Table {
    fn score(&self, item: u32, subject: &PalwEvalSubjectV1, kind: PalwScoringKindV1) -> Option<i64> {
        self.0.get(&(item, *subject, kind as u8)).copied()
    }
    fn any_score(&self, item: u32, kind: PalwScoringKindV1) -> bool {
        self.0.keys().any(|(i, _, k)| *i == item && *k == kind as u8)
    }
}

fn promotion() -> Value {
    let rule = PalwImproveRuleV1 {
        n_min: 4,
        delta_permille: 100,
        epsilon_permille: 100,
        epsilon_safety_permille: 0,
        alpha_permille: 50,
        has_judge: false,
        has_pairwise: false,
    };
    let (a, b, c) = (h(0xA0), h(0xB0), h(0xC0));
    let subjects = [PalwEvalSubjectV1::Candidate(a), PalwEvalSubjectV1::Candidate(b), PalwEvalSubjectV1::Candidate(c)];
    let hold = |i: u32| PalwEvalItemV1 {
        item: i,
        case_id: h(i as u8),
        source: PalwItemSourceV1::HoldOut,
        supplier: None,
        seed: h(0),
        judge: None,
        dropped: false,
    };
    let mut items: Vec<PalwEvalItemV1> = (0..10u32).map(hold).collect();
    items.push(PalwEvalItemV1 {
        item: 10,
        case_id: h(10),
        source: PalwItemSourceV1::Regression { index: 0 },
        supplier: None,
        seed: h(0),
        judge: None,
        dropped: false,
    });
    items.push(hold(11));
    items[9].dropped = true;
    let mut table = Table::default();
    let parent = PalwEvalSubjectV1::Parent;
    let em = PalwScoringKindV1::ExactMatch as u8;
    // Items 0..=8: the parent fails; A and C pass all; B passes the even ones and ties the odd ones.
    // Item 9 is dropped (everyone's scores ignored). Item 11: the parent's score is missing (a parent
    // win for everyone); B's is missing too. Item 10 is the regression suite: A regresses on it.
    for i in 0..9u32 {
        table.0.insert((i, parent, em), 0);
        table.0.insert((i, subjects[0], em), 1);
        table.0.insert((i, subjects[1], em), (i % 2 == 0) as i64);
        table.0.insert((i, subjects[2], em), 1);
    }
    for s in &subjects {
        table.0.insert((9, *s, em), 1);
    }
    table.0.insert((11, subjects[0], em), 1);
    table.0.insert((11, subjects[2], em), 1);
    table.0.insert((10, parent, PalwScoringKindV1::RefLogLik as u8), -100);
    table.0.insert((10, subjects[0], PalwScoringKindV1::RefLogLik as u8), -200);
    table.0.insert((10, subjects[1], PalwScoringKindV1::RefLogLik as u8), -100);
    table.0.insert((10, subjects[2], PalwScoringKindV1::RefLogLik as u8), -50);
    let k = subjects.len() as u32;
    let counted: Vec<(Hash64, _)> = subjects
        .iter()
        .map(|s| {
            let PalwEvalSubjectV1::Candidate(class) = s else { unreachable!() };
            let mut counts = palw_improve_counts_v1(&table, &items, s, &rule, &|_| false);
            counts.eligible = palw_improve_eligible_v1(&counts, &rule, k);
            (*class, counts)
        })
        .collect();
    let decision = palw_improve_decide_v1(&counted, &rule);
    let rows: Vec<Value> = counted
        .iter()
        .map(|(class, c)| {
            json!({
                "candidate": hex(class),
                "primary": [c.primary.wins, c.primary.losses, c.primary.ties],
                "regression": [c.regression.wins, c.regression.losses, c.regression.ties],
                "eligible": c.eligible,
            })
        })
        .collect();
    json!({
        "rule": { "n_min": 4, "delta_permille": 100, "epsilon_permille": 100, "epsilon_safety_permille": 0, "alpha_permille": 50, "K": k },
        "scenario": "items 0–8 and 11 primary (the parent fails 0–8 and has no score on 11; A and C pass all; B passes the even ones, ties the odd ones, and has no score on 11); item 9 dropped; item 10 regression (A −200 vs −100, B tie, C better)",
        "counts": rows,
        "decision": format!("{decision:?}"),
    })
}

#[test]
fn the_improvement_vectors_are_the_code_s() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/improve-v1");
    let bless = std::env::var("IMPROVE_BLESS").is_ok_and(|v| v == "1");
    if bless {
        std::fs::create_dir_all(&dir).expect("the vector directory");
    }
    for (name, value) in [
        ("sign-table", sign_table()),
        ("policy", policy()),
        ("material", material()),
        ("draw", draw()),
        ("suite", suite()),
        ("judge", judge()),
        ("promotion", promotion()),
    ] {
        let bytes = serde_json::to_string_pretty(&value).expect("json") + "\n";
        let path = dir.join(format!("{name}.json"));
        if bless {
            std::fs::write(&path, &bytes).expect("written");
        } else {
            let on_disk =
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e} (IMPROVE_BLESS=1 writes it)", path.display()));
            assert!(on_disk == bytes, "{} differs from the code's (IMPROVE_BLESS=1 rewrites it)", path.display());
        }
    }
    // The decision, spelled out: A regresses past ε on the suite; B's 5–1 is under the sign test at
    // α/K = 5 %/3 (k(6) = 6); C's 9–1 passes (k(10) = 9) and wins.
    let p = promotion();
    let counts = p["counts"].as_array().unwrap();
    assert_eq!(counts[0]["primary"], json!([9, 1, 0]));
    assert_eq!(counts[1]["primary"], json!([5, 1, 4]));
    assert_eq!(counts[2]["primary"], json!([9, 1, 0]));
    assert_eq!(
        [counts[0]["eligible"].clone(), counts[1]["eligible"].clone(), counts[2]["eligible"].clone()],
        [json!(false), json!(false), json!(true)]
    );
    assert!(p["decision"].as_str().unwrap().starts_with("Promoted"));
}
