//! **AUDIT B2 (re-run) — is ADR-0143's "one owner per artifact root" a rule about a ROOT?**
//!
//! Read-only audit artefact. Nothing here is a fixture any shipped code reads.
//!
//! `palw_reward_properties_v1::no_registrant_writable_field_increases_the_weight` states the
//! closure this file tests: "a copy must name the artifact root it is a tiling of, and past
//! ADR-0143 a root has exactly one owner", and it checks that closure with a `str::contains`
//! over `config/params.rs`. These tests drive the real fold instead.

use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2 as Obj, PalwPwuRuleV2, PalwStateParamsV2,
    PalwStateV2Error, PalwTransitionExtrasV1, apply_palw_transition_v2_with_extras,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}
fn bond(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
}

const BASE: u64 = 0xBA5E;
const FLOOR_ROOT: u64 = 0xF100;
/// One artifact. One inventory root. Several class ids over it.
const ROOT: u64 = 0xA271_FAC7;

fn params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, 20, 500, 1_000, h(BASE), 4, 1_000, 100, 1_000, 0)
        .expect("state params")
        .with_claim_retirement_daa(3_000)
        .expect("retirement")
        .with_min_base_class_share_permille(20)
        .expect("floor reserve")
}

fn registration(class: u64, root: u64, share: u16) -> Obj {
    Obj::ClassRegistered {
        class_id: h(class),
        artifact_root: h(root),
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 7_708 },
        initial_target: u128::MAX / 2,
        share_permille: share,
        activation_daa: 0,
        admission: None,
    }
}

fn bond_object(n: u64) -> Obj {
    Obj::BondRegistered {
        bond: bond(n),
        pubkey: vec![n as u8; 32],
        operator_pubkey: vec![(n as u8).wrapping_add(21); 8],
        collateral: 1_000_000_000_000_000,
        payout_payload: h(0x9A4 + n),
        capable_classes: Default::default(),
        signature: vec![9u8; 64],
    }
}

struct Chain {
    state: PalwChainStateV2,
    params: PalwStateParamsV2,
    daa: u64,
}

impl Chain {
    /// Genesis carries two bonds and the liveness floor, the way the t12 card does.
    fn new() -> Self {
        let mut c = Chain { state: PalwChainStateV2::genesis(), params: params(), daa: 0 };
        c.try_step(&[bond_object(0xB0), bond_object(0xB1), registration(BASE, FLOOR_ROOT, 1_000)]).expect("genesis folds");
        c
    }

    fn try_step(&mut self, objects: &[Obj]) -> Result<(), PalwStateV2Error> {
        self.daa += 1;
        let ctx =
            PalwBlockContextV2 { block: h(self.daa | 0x1000_0000), daa_score: self.daa, blue_score: self.daa, subsidy: 1_000_000 };
        // ADR-0143 armed, exactly as testnet-12 arms it — at DAA 0.
        let extras = PalwTransitionExtrasV1 { artifact_root_ownership_active: true, model_lines_active: true, ..Default::default() };
        let (next, _) =
            apply_palw_transition_v2_with_extras(&self.state, &self.params, &ctx, objects, None, false, false, false, false, &extras)?;
        self.state = next;
        Ok(())
    }
}

/// **The measurement: how many distinct class rows one artifact root carries, with ADR-0143 armed.**
#[test]
fn b2x_one_artifact_root_carries_as_many_classes_as_the_registrant_mints_ids() {
    let mut c = Chain::new();

    let mut accepted = Vec::new();
    for k in 0..8u64 {
        let class = 0xC0DE_0000 + k;
        match c.try_step(&[registration(class, ROOT, 1)]) {
            Ok(()) => accepted.push(class),
            Err(e) => {
                println!("class #{k} over root {ROOT:#x} REFUSED: {e}");
                break;
            }
        }
    }

    println!("\nADR-0143 armed (artifact_root_ownership_active = true)");
    println!("one artifact root {:#x}, distinct class ids accepted: {}", ROOT, accepted.len());
    for class in accepted.iter() {
        let owner = c.state.artifact_owner_of(&h(*class), &h(ROOT)).copied();
        let share = c.state.class_share_permille(&h(*class)).unwrap_or(0);
        println!("  class {:#x}: owner row {:?}  share {share}permille", class, owner.map(|o| (o.line_id, o.version)));
    }
    println!("floor {:#x} share {}permille", BASE, c.state.class_share_permille(&h(BASE)).unwrap_or(0));
    println!("artifact_owners rows written: {}", c.state.artifact_owners_iter().count());

    assert!(accepted.len() > 1, "if this fails, the ownership index really is keyed by the root alone");
}

/// **The control: the rule DOES fire — but only inside one class id.**
///
/// `claim_artifact_root` looks the root up at `(class_id, root)`, so the refusal
/// `DuplicateArtifactRoot` is reachable only when the SAME class id is presented with a second
/// line. A second class id is a second key.
#[test]
fn b2x_duplicate_artifact_root_only_fires_within_one_class_id() {
    let mut c = Chain::new();
    c.try_step(&[registration(0xC0DE, ROOT, 1)]).expect("the first class over the root");

    // Same class id, a stranger's line over the class's founding root: refused.
    let squat = Obj::ModelLineFounded {
        class_id: h(0xC0DE),
        name: b"COPY".to_vec(),
        founder: bond(0xB1),
        root: h(ROOT),
        signature: vec![1],
    };
    let within = c.try_step(&[squat]);
    println!("same class id, second line over the same root => {within:?}");
    assert!(matches!(within, Err(PalwStateV2Error::DuplicateArtifactRoot { .. })), "the rule fires within one class id");

    // A different class id over the very same root: accepted.
    let across = c.try_step(&[registration(0xC0DF, ROOT, 1)]);
    println!("different class id, same root      => {:?}", across.as_ref().map(|_| "ACCEPTED"));
    assert!(across.is_ok(), "and does not fire across class ids");

    println!(
        "\nartifact_owners keys now: {:?}",
        c.state.artifact_owners_iter().map(|((class, root), o)| (*class, *root, o.version)).collect::<Vec<_>>()
    );
}

/// **Under testnet-12's own rule set** — `palw_admission_independence` armed at DAA 0, so a bought
/// registration must ask for zero share. The duplicates are still admitted; the ownership index
/// still writes one row per class id.
#[test]
fn b2x_the_t12_rule_set_does_not_change_the_answer() {
    let mut c = Chain { state: PalwChainStateV2::genesis(), params: params(), daa: 0 };
    let extras = || PalwTransitionExtrasV1 {
        artifact_root_ownership_active: true,
        model_lines_active: true,
        admission_independence_daa: Some(0),
        ..Default::default()
    };
    // Genesis (the floor's 1000permille is the exempt arm: no carriage, so it is not "bought").
    c.daa += 1;
    let ctx = PalwBlockContextV2 { block: h(1 | 0x1000_0000), daa_score: 1, blue_score: 1, subsidy: 1_000_000 };
    let (next, _) = apply_palw_transition_v2_with_extras(
        &c.state,
        &c.params,
        &ctx,
        &[bond_object(0xB0), registration(BASE, FLOOR_ROOT, 1_000)],
        None,
        false,
        false,
        false,
        false,
        &extras(),
    )
    .expect("genesis folds under the t12 rule set");
    c.state = next;

    let mut ok = 0;
    for k in 0..8u64 {
        c.daa += 1;
        let ctx =
            PalwBlockContextV2 { block: h(c.daa | 0x1000_0000), daa_score: c.daa, blue_score: c.daa, subsidy: 1_000_000 };
        match apply_palw_transition_v2_with_extras(
            &c.state,
            &c.params,
            &ctx,
            &[registration(0xC0DE_0000 + k, ROOT, 0)],
            None,
            false,
            false,
            false,
            false,
            &extras(),
        ) {
            Ok((next, _)) => {
                c.state = next;
                ok += 1;
            }
            Err(e) => {
                println!("t12 rule set: class #{k} over the shared root refused: {e}");
                break;
            }
        }
    }
    println!("\nt12 rule set (admission_independence @0, ADR-0143 armed)");
    println!("duplicate class rows accepted over one artifact root: {ok}");
    println!("artifact_owners rows: {}", c.state.artifact_owners_iter().count());
    assert!(ok > 1);
}

/// **The pair itself: two class ids, one model, one artifact, one price.**
///
/// `n_threads` is the executor's own thread count. `validate_shape` demands
/// `flash_attn_disabled == 1` precisely so the thread count cannot change an answer, and the
/// economic cost table never reads it — but `shape_profile_id` is canonical Borsh over the whole
/// struct, so it mints a second class id.
#[test]
fn b2x_the_minimal_duplicate_pair_prices_identically() {
    use kaspa_consensus_core::palw_base0_profile::rc_job_context;
    use kaspa_consensus_core::palw_canonical_work_v1::PalwCanonicalClassDescriptorV1;
    use kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1;
    use kaspa_consensus_core::palw_qwen25_profile::{
        PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1,
    };
    use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};

    const N_CTX: u32 = 2_097_152;
    // testnet-12: escrow = calc_block_subsidy(0) x worker_carve 720/1000, rate 900_000_000/1e9.
    const ESCROW_SOMPI: u64 = 320_084_650_080;
    const RATE: u64 = 900_000_000;

    let honest = qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: N_CTX, ..QWEN25_1_5B }).expect("row");
    let mut twin = honest.clone();
    twin.n_threads = honest.n_threads.wrapping_add(1).max(1);
    honest.validate_shape().expect("honest is a legal shape");
    twin.validate_shape().expect("the twin is a legal shape too");

    let (pf, de) = qwen25_a16_held_canonical_v1(N_CTX);
    let jh = rc_job_context(&honest, pf, de);
    let jt = rc_job_context(&twin, pf, de);
    let wh = palw_model_work_from_carriage_v1(&honest, &jh).expect("work");
    let wt = palw_model_work_from_carriage_v1(&twin, &jt).expect("work");
    let cid = |p: &kaspa_consensus_core::palw_step::PalwShapeProfileV3| {
        PalwCanonicalClassDescriptorV1::of(p, Hash64::default()).expect("one weight format").canonical_class_id_v1()
    };
    let w0 = palw_work_floor_v1(ESCROW_SOMPI, RATE);

    println!("\nt12 dense row Qwen2.5-1.5B A16 graph-v7 @ n_ctx {N_CTX}, canonical ({pf},{de})");
    println!("                        honest                                twin (n_threads +1)");
    println!("shape_profile_id (CHAIN CLASS ID)");
    println!("  {}\n  {}", honest.shape_profile_id(), twin.shape_profile_id());
    println!("canonical_class_id_v1 (ADR-0145 economic identity)");
    println!("  {}\n  {}", cid(&honest), cid(&twin));
    println!("economic_ccu_per_claim  {} vs {}", wh.economic_ccu_per_claim, wt.economic_ccu_per_claim);
    println!("verification_ccu        {} vs {}", wh.verification_ccu, wt.verification_ccu);
    println!("W0                      {w0} MAC-eq");
    println!(
        "work ticket target      {} vs {}   (u128::MAX = {})",
        palw_work_ticket_target_v1(wh.economic_ccu_per_claim, w0),
        palw_work_ticket_target_v1(wt.economic_ccu_per_claim, w0),
        u128::MAX
    );
    println!(
        "reachable kernel sets equal: {}",
        kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&honest)
            == kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&twin)
    );

    assert_ne!(honest.shape_profile_id(), twin.shape_profile_id(), "two chain class ids");
    assert_eq!(cid(&honest), cid(&twin), "one model");
    assert_eq!(wh.economic_ccu_per_claim, wt.economic_ccu_per_claim, "one price");
    assert_eq!(
        palw_work_ticket_target_v1(wh.economic_ccu_per_claim, w0),
        palw_work_ticket_target_v1(wt.economic_ccu_per_claim, w0),
        "one lottery target"
    );
}
