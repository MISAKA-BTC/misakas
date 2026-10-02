//! **RFC-0006, the node's half, through the doors kaspad uses.** A tiny IR class (the golden `dense-gqa-2layer` program, two
//! layers) is written as a `PALWTIR1` container and loaded through the SDK's one door into the node's backend registry; an
//! honest producer's capture and a producer's planted lie are run through the seat's own functions
//! ([`palw_tir_shard_outcome_v1`], [`palw_tir_shard_accusation_v1`]):
//!
//! * **every duty of an honest claim verifies** — each shard of a 2-shard plan, under `S_P = 1` (the whole shard) and `S_P = 2`
//!   (a segment each), for a class seat and for the shard's outsider (the whole shard);
//! * **a planted lie is found by the seat that owns it**, at that leaf, and its accusation is the one the chain's one-move gate
//!   convicts on (the same `palw_tir_one_move_accusation_to_file_v1` the whole-job replay files through);
//! * **a duty whose capture is a fold abstains by name**, and a flat panel's duty is no sharded duty.
//!
//! The wire and the pool are tested beside it: a V4 receipt queues and nothing else decodes as one.

use super::tir_shard::{
    PalwTirShardOutcomeV1, palw_tir_shard_accusation_v1, palw_tir_shard_arrival_push_v1, palw_tir_shard_outcome_v1,
};
use crate::palw_backends::PalwBackendRegistry;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3};
use kaspa_consensus_core::palw_producer_v2::{PalwSeatDutyV2, PalwTirShardDutyV1};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_tir_shard_v1::PalwSeatReceiptV4;
use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use misaka_palw_sdk::lineages::tir::TirBackendV1;
use std::path::PathBuf;

const NETWORK: &[u8] = b"misaka-palw-rc";
const LADDER: u64 = 1 << 26;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

struct Ir {
    dir: PathBuf,
    registry: PalwBackendRegistry,
    class_id: Hash64,
    root: Hash64,
}

impl Drop for Ir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

impl Ir {
    fn tir(&self) -> TirBackendV1 {
        self.registry.resolve_tir_v1(self.class_id, self.root).expect("an IR class").expect("its backend")
    }
}

/// The class: `dense-gqa-2layer` under the tiled logits scheme, 8-lane commit tiles, two-position checkpoints and four-row
/// history tiles (so `G = 4`), loaded through the SDK's door.
fn shard_class(tag: &str) -> Ir {
    use kaspa_consensus_core::palw_step_refute::{PALW_LOGITS_TILE_LANES, tiled_logits_scheme_id_v1};
    use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
    use misaka_palw_tir::TirProgramV1;
    let vector = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1/programs/dense-gqa-2layer.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&vector).expect("the golden program")).expect("json");
    let mut program = TirProgramV1::decode_canonical(&unhex(v["program_borsh_hex"].as_str().unwrap())).expect("canonical");
    program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    let program = TirProgramV1::decode_canonical(&program.encode()).expect("still canonical");
    let mut tensors = std::collections::BTreeMap::new();
    for t in v["params"].as_array().unwrap() {
        tensors.insert(
            (t["param"].as_u64().unwrap() as u16, t["layer"].as_u64().map(|l| l as u16)),
            unhex(t["le_hex"].as_str().unwrap()),
        );
    }
    let mut commit_tiles = Vec::new();
    for (bi, b) in program.blocks.iter().enumerate() {
        for (ni, n) in b.nodes.iter().enumerate() {
            if n.commit {
                let logits = bi == program.schedule.post as usize && ni == program.logits as usize;
                commit_tiles.push(if logits { PALW_LOGITS_TILE_LANES as u32 } else { 8 });
            }
        }
    }
    let layout = PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context: 64,
        checkpoint_interval: 2,
        h_tile: 4,
        commit_tiles,
        state_tiles: program.states.iter().map(|_| 4).collect(),
    };
    let dir = std::env::temp_dir().join(format!("kaspad-tir-shard-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tiny.palwtir");
    let meta = serde_json::json!({ "model_id": format!("test/tiny-ir-shard-{tag}") }).to_string();
    misaka_palw_tir_artifact::write_container_v1(&path, &program, borsh::to_vec(&layout).unwrap(), [9; 64], meta, &mut |j, l| {
        tensors.get(&(j, l)).cloned().ok_or_else(|| format!("no tensor {j} {l:?}"))
    })
    .expect("the container");
    let court = PalwCourtParamsV2::new(LADDER, 20, 2).expect("a court");
    let form = PalwPromptIdsFormV1::Flat;
    let sdk = misaka_palw_sdk::PalwClassSdk::builtin_v1(court, form, NETWORK.to_vec());
    let holding = sdk.load_artifact(&path).expect("the IR lineage loads it by its magic");
    let registry = PalwBackendRegistry::new(court, form, vec![holding], NETWORK.to_vec());
    let entry = misaka_palw_sdk::tir_registration::tir_entries_of_v1(registry.holdings()).remove(0);
    Ir { dir, class_id: entry.class_id(), root: entry.artifact_root, registry }
}

fn bond(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}

/// A duty at `shard` of `s_l`, assigned `segments` of `s_p`; `outsider` seats attest the whole shard.
fn duty(ir: &Ir, shard: u16, s_l: u16, s_p: u16, segments: PalwSegmentMaskV2, outsider: bool) -> PalwSeatDutyV2 {
    PalwSeatDutyV2 {
        accepted_block: Hash64::from_u64_word(0xB10C),
        claim_id: Hash64::from_u64_word(0xC1A1),
        class_id: ir.class_id,
        artifact_root: ir.root,
        seat_bond: bond(7),
        executor_bond: bond(1),
        execution_root: Hash64::default(),
        trace_root: Hash64::default(),
        output_root: Hash64::default(),
        bound_daa: 100,
        receipt_deadline: 700,
        panel_anchor: Hash64::from_u64_word(0xA9),
        seat_index: 1,
        panel_seat_count: u16::from(s_l) * 4,
        pwu: 0,
        quanta: 0,
        free_prompt: false,
        work_leaves: 0,
        job_identity: Hash64::default(),
        tir_shard: Some(PalwTirShardDutyV1 { shard, s_l, s_p, outsider, slice_index: u8::from(!outsider), segments }),
    }
}

fn run(tir: &TirBackendV1, material: &[u8], d: &PalwSeatDutyV2) -> PalwTirShardOutcomeV1 {
    palw_tir_shard_outcome_v1(tir, material, d, &mut kaspa_cpu()).expect("the duty runs")
}

fn kaspa_cpu() -> misaka_palw_sdk::lineages::tir::CpuKernelBackendV1 {
    misaka_palw_sdk::lineages::tir::CpuKernelBackendV1
}

#[test]
fn every_duty_of_an_honest_claim_verifies_for_class_seats_and_the_outsider() {
    let ir = shard_class("honest");
    let tir = ir.tir();
    let (job, prompt) = tir.job_for_anchor(Hash64::from_bytes([0x3C; 64])).unwrap();
    let honest = tir.execute(&job, &prompt).unwrap();
    let mut verified = 0;
    for (s_p, masks) in [(1u16, vec![PalwSegmentMaskV2::full(1)]), (2, vec![PalwSegmentMaskV2::single(0), PalwSegmentMaskV2::single(1), PalwSegmentMaskV2::full(2)])] {
        for shard in 0..2 {
            for (i, mask) in masks.iter().enumerate() {
                let d = duty(&ir, shard, 2, s_p, *mask, i == masks.len() - 1);
                match run(&tir, &honest.material, &d) {
                    PalwTirShardOutcomeV1::Valid { .. } => verified += 1,
                    other => panic!("S_P {s_p} shard {shard} mask {:#x}: {other:?}", mask.0),
                }
            }
        }
    }
    assert_eq!(verified, 8);
}

#[test]
fn a_planted_lie_is_found_by_the_seat_that_owns_it_and_its_accusation_convicts() {
    use kaspa_consensus_core::palw_producer_v2::PalwDisputableClaimV2;
    let ir = shard_class("lie");
    let tir = ir.tir();
    let (job, prompt) = tir.job_for_anchor(Hash64::from_bytes([0x3C; 64])).unwrap();
    let honest = tir.execute(&job, &prompt).unwrap();
    let capture = tir.decode_capture(&honest.material).unwrap();
    let leaves = capture.binding.step_leaf_count;
    let (mut found_inside, mut accused) = (0usize, 0usize);
    for leaf in [leaves / 5, leaves / 3, leaves / 2, leaves - 3] {
        let forged = tir.execute_with_injected_fault(&job, &prompt, leaf).unwrap();
        let mut faulting = Vec::new();
        for shard in 0..2u16 {
            let d = duty(&ir, shard, 2, 1, PalwSegmentMaskV2::full(1), false);
            if let PalwTirShardOutcomeV1::Fault { leaf: found, row, position } = run(&tir, &forged.material, &d) {
                faulting.push((shard, found, row, position));
                // The accusation the finding builds is one the chain's one-move gate convicts on.
                let target = PalwDisputableClaimV2 {
                    accepted_block: d.accepted_block,
                    claim_id: d.claim_id,
                    class_id: ir.class_id,
                    artifact_root: ir.root,
                    executor_bond: d.executor_bond,
                    trace_root: forged.trace_root,
                    execution_root: forged.execution_root,
                    licensed_daa: 100,
                    free_prompt: false,
                };
                let court = PalwCourtParamsV2::new(LADDER, 20, 2).unwrap();
                let rules = tir.court_rules(&court);
                let built = palw_tir_shard_accusation_v1(
                    &tir,
                    &forged.material,
                    found,
                    row,
                    &target,
                    bond(9),
                    &court,
                    &rules,
                    LADDER,
                    PalwPromptIdsFormV1::Flat,
                )
                .expect("the accusation builds");
                if built.is_some() {
                    accused += 1;
                }
            }
        }
        assert!(!faulting.is_empty(), "leaf {leaf}: one of the two shards owns it");
        // The finding names a leaf at or before the planted one inside the owner's cells (the first divergent leaf).
        if let Some((_, Some(found), _, _)) = faulting.first() {
            assert!(*found <= leaf || faulting.len() > 1, "leaf {leaf}: found {found}");
            found_inside += 1;
        }
    }
    assert!(found_inside >= 3, "{found_inside}");
    assert!(accused >= 3, "the chain's gate convicts the finding: {accused}");
}

#[test]
fn a_fold_abstains_by_name_and_a_flat_duty_is_no_shard_duty() {
    let ir = shard_class("fold");
    let tir = ir.tir();
    let (job, prompt) = tir.job_for_anchor(Hash64::from_bytes([0x3C; 64])).unwrap();
    let folding = TirBackendV1::new(
        "fold".into(),
        tir.artifact().clone(),
        ir.root,
        tir.canonical().clone(),
        PalwPromptIdsFormV1::Flat,
        LADDER,
    )
    .unwrap()
    .with_dense_capture_bytes(0);
    let fold = folding.execute(&job, &prompt).unwrap();
    let d = duty(&ir, 0, 2, 1, PalwSegmentMaskV2::full(1), false);
    assert!(matches!(run(&folding, &fold.material, &d), PalwTirShardOutcomeV1::Abstain(why) if why.contains("fold")));
    let flat = PalwSeatDutyV2 { tir_shard: None, ..d };
    let honest = tir.execute(&job, &prompt).unwrap();
    assert!(palw_tir_shard_outcome_v1(&tir, &honest.material, &flat, &mut kaspa_cpu()).is_err());
}

#[test]
fn a_cell_masked_receipt_queues_and_no_other_version_decodes_as_one() {
    let v2 = PalwSeatReceiptV2 {
        claim: Hash64::from_u64_word(1),
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: bond(7),
        signed_daa: 120,
        signature: vec![5u8; kaspa_txscript::MLDSA87_SIG_LEN],
    };
    let v4 = PalwSeatReceiptV4 { receipt: v2.clone(), shard: 1, segments: PalwSegmentMaskV2::single(1) };
    let v3 = PalwSeatReceiptV3 { receipt: v2.clone(), segments: PalwSegmentMaskV2::single(1) };
    let mut queue = std::collections::VecDeque::new();
    assert!(palw_tir_shard_arrival_push_v1(&mut queue, &borsh::to_vec(&v4).unwrap()));
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0], v4);
    // V2 and V3 bytes are not V4 receipts: the flat drain takes them.
    assert!(!palw_tir_shard_arrival_push_v1(&mut queue, &borsh::to_vec(&v2).unwrap()));
    assert!(!palw_tir_shard_arrival_push_v1(&mut queue, &borsh::to_vec(&v3).unwrap()));
    assert!(!palw_tir_shard_arrival_push_v1(&mut queue, b"junk"));
    assert_eq!(queue.len(), 1);
    // A V4 receipt whose signature is not an ML-DSA-87 signature's length is refused at the drain (no key verifies it).
    let mut short = v4.clone();
    short.receipt.signature = vec![1, 2, 3];
    assert!(palw_tir_shard_arrival_push_v1(&mut queue, &borsh::to_vec(&short).unwrap()), "recognised as a V4 receipt");
    assert_eq!(queue.len(), 1, "and not queued");
    // And the V4 bytes are no V3 or V2 receipt.
    assert!(borsh::from_slice::<PalwSeatReceiptV3>(&borsh::to_vec(&v4).unwrap()).is_err());
    assert!(borsh::from_slice::<PalwSeatReceiptV2>(&borsh::to_vec(&v4).unwrap()).is_err());
}
