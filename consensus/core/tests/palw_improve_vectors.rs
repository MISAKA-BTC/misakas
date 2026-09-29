//! **Spec 17 §17.13: the Model Improvement Protocol's golden vectors** (`consensus-vectors/improve-v1/`).
//!
//! Regenerated from the pure functions every implementation must agree on — the pinned sign table,
//! the policy digest and message, the material tree, the epoch seed and the draw, the suite items, the
//! judge draw and the pairwise order, and the promotion rule over score tables — and compared byte for
//! byte with the files. `IMPROVE_BLESS=1` rewrites them, which is a change of the semantics and is
//! reviewed as one. (`transitions.json` and `pool.json` come from the fold's own suite.)

use std::collections::BTreeMap;
use std::path::PathBuf;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_improve_epoch_v1::{
    PalwDrawEntryV1, PalwMaterialKindV1, palw_improve_dataset_root_v1, palw_improve_draw_key_v1, palw_improve_draw_v1,
    palw_improve_epoch_seed_v1, palw_improve_judge_index_v1, palw_improve_material_append_v1, palw_improve_material_leaf_v1,
    palw_improve_material_root_v1, palw_improve_pair_order_v1, palw_improve_setter_item_id_v1, palw_improve_suite_item_id_v1,
};
use kaspa_consensus_core::palw_improve_policy_v1::{
    palw_improvement_policy_check_v1, palw_improvement_policy_example_v1, palw_improvement_policy_message_v1,
};
use kaspa_consensus_core::palw_improve_promotion_v1::{
    PALW_IMPROVE_SIGN_ALPHAS_PERMILLE_V1, PALW_IMPROVE_SIGN_TABLE_ID_HEX_V1, PalwImproveRuleV1, PalwImproveScoresV1,
    palw_improve_counts_v1, palw_improve_decide_v1, palw_improve_eligible_v1, palw_improve_sign_critical_v1,
};
use kaspa_consensus_core::palw_improve_state_v1::{
    PalwEvalItemV1, PalwEvalSubjectV1, PalwImprovementPolicyV1, PalwItemSourceV1, PalwMaterialFrontierV1, PalwScoringKindV1,
    palw_improvement_policy_digest_v1,
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

fn policy() -> Value {
    let example = palw_improvement_policy_example_v1();
    let domain = h(0xA1);
    let line = h(0xA2);
    let edits: Vec<(&str, Box<dyn Fn(&mut PalwImprovementPolicyV1)>)> = vec![
        ("the example", Box::new(|_| {})),
        ("w_eval = beacon_delay + 32 (E12)", Box::new(|p| p.windows.w_eval = p.windows.beacon_delay + 32)),
        ("w_eval = beacon_delay + 33", Box::new(|p| p.windows.w_eval = p.windows.beacon_delay + 33)),
        ("grid below L_e", Box::new(|p| p.windows.grid = 900)),
        ("α = 25 (untabled)", Box::new(|p| p.eval.alpha_permille = 25)),
        ("α = 10", Box::new(|p| p.eval.alpha_permille = 10)),
        ("k_max = 9 (past the drill's 8)", Box::new(|p| p.k_max = 9)),
        ("n_min > n", Box::new(|p| p.eval.n_min = p.eval.n + 1)),
        ("no primary stage", Box::new(|p| p.eval.stages.clear())),
    ];
    let checks: Vec<Value> = edits
        .into_iter()
        .map(|(name, edit)| {
            let mut p = example.clone();
            edit(&mut p);
            json!({ "case": name, "accepted": palw_improvement_policy_check_v1(&p, &PALW_DRILL_IMPROVE_CEILINGS_V1).is_ok() })
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
        "suite_item_0_of_root_E1": hex(&palw_improve_suite_item_id_v1(&h(0xE1), 0)),
        "judge_of_item": (0..8u32).map(|i| palw_improve_judge_index_v1(&seed, i, judges.len()).unwrap()).collect::<Vec<_>>(),
        "pair_order_of_item_for_candidate_F1": (0..8u32).map(|i| palw_improve_pair_order_v1(&seed, i, &h(0xF1))).collect::<Vec<_>>(),
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
    for (name, value) in
        [("sign-table", sign_table()), ("policy", policy()), ("material", material()), ("draw", draw()), ("promotion", promotion())]
    {
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
