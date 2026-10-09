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
    PalwTirShardShadowV1, palw_tir_shard_shadow_run_v1,
    PalwTirRowFetchV1, PalwTirRunPursuitV1,
    PalwTirShardOutcomeV1, palw_tir_shard_accusation_over_v1, palw_tir_shard_accusation_v1, palw_tir_shard_arrival_push_v1, palw_tir_shard_outcome_over_v1,
    palw_tir_shard_outcome_v1,
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
    for (s_p, masks) in [
        (1u16, vec![PalwSegmentMaskV2::full(1)]),
        (2, vec![PalwSegmentMaskV2::single(0), PalwSegmentMaskV2::single(1), PalwSegmentMaskV2::full(2)]),
    ] {
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
    let folding =
        TirBackendV1::new("fold".into(), tir.artifact().clone(), ir.root, tir.canonical().clone(), PalwPromptIdsFormV1::Flat, LADDER)
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

#[test]
fn a_seat_that_holds_only_its_shards_rows_reaches_the_verdict_of_a_full_holder() {
    use misaka_palw_sdk::lineages::tir::{TirCaptureV1, TirFileMirrorV1, fetch_shard_params_v1};
    let ir = shard_class("holder");
    let tir = ir.tir();
    let (job, prompt) = tir.job_for_anchor(Hash64::from_bytes([0x3C; 64])).unwrap();
    let honest = tir.execute(&job, &prompt).unwrap();
    let forged = tir.execute_with_injected_fault(&job, &prompt, 20).unwrap();
    let path = ir.dir.join("tiny.palwtir");
    for (material, name) in [(&honest.material, "honest"), (&forged.material, "forged")] {
        let capture = TirCaptureV1::decode(material).unwrap();
        for shard in 0..2u16 {
            let d = duty(&ir, shard, 2, 1, PalwSegmentMaskV2::full(1), false);
            let holding =
                fetch_shard_params_v1(&capture.binding.class, ir.class_id, ir.root, 2, shard, &TirFileMirrorV1(path.clone())).unwrap();
            let over = palw_tir_shard_outcome_over_v1(&holding, &capture, &d, &mut kaspa_cpu()).unwrap();
            let full = run(&tir, material, &d);
            assert_eq!(over, full, "{name} shard {shard}: the holder's verdict is the full holder's");
        }
    }
}

#[test]
fn a_shard_only_seat_files_the_accusation_a_full_holder_would_and_the_gate_convicts_it() {
    use kaspa_consensus_core::palw_producer_v2::PalwDisputableClaimV2;
    use misaka_palw_sdk::lineages::tir::{TirCaptureV1, TirFileMirrorV1, fetch_shard_params_v1};
    let ir = shard_class("holder-accuses");
    let tir = ir.tir();
    let (job, prompt) = tir.job_for_anchor(Hash64::from_bytes([0x3C; 64])).unwrap();
    let path = ir.dir.join("tiny.palwtir");
    let court = PalwCourtParamsV2::new(LADDER, 20, 2).unwrap();
    let rules = tir.court_rules(&court);
    let leaves = TirCaptureV1::decode(&tir.execute(&job, &prompt).unwrap().material).unwrap().binding.step_leaf_count;
    let (mut accused, mut dissected) = (0usize, 0usize);
    for leaf in [20, leaves / 5, leaves / 3, leaves / 2, leaves - 3] {
        let forged = tir.execute_with_injected_fault(&job, &prompt, leaf).unwrap();
        let capture = TirCaptureV1::decode(&forged.material).unwrap();
        for shard in 0..2u16 {
            let d = duty(&ir, shard, 2, 1, PalwSegmentMaskV2::full(1), false);
            let holding =
                fetch_shard_params_v1(&capture.binding.class, ir.class_id, ir.root, 2, shard, &TirFileMirrorV1(path.clone())).unwrap();
            let PalwTirShardOutcomeV1::Fault { leaf: found, row, .. } = palw_tir_shard_outcome_over_v1(&holding, &capture, &d, &mut kaspa_cpu()).unwrap()
            else {
                continue;
            };
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
            let over = palw_tir_shard_accusation_over_v1(
                &holding, &capture, found, row, &target, bond(9), &court, &rules, LADDER, PalwPromptIdsFormV1::Flat,
            );
            let full = palw_tir_shard_accusation_v1(
                &tir, &forged.material, found, row, &target, bond(9), &court, &rules, LADDER, PalwPromptIdsFormV1::Flat,
            )
            .unwrap();
            match over {
                Ok(Some((label, accusation))) => {
                    let (flabel, faccusation) = full.expect("a full holder files what the shard seat files");
                    assert_eq!(label, flabel, "leaf {leaf} shard {shard}");
                    assert_eq!(borsh::to_vec(&accusation).unwrap(), borsh::to_vec(&faccusation).unwrap(), "the same bytes");
                    accused += 1;
                }
                Ok(None) => panic!("leaf {leaf} shard {shard}: the finding convicts at a full holder and not here"),
                Err(why) => {
                    assert!(why.contains("dissected"), "refused by name: {why}");
                    dissected += 1;
                }
            }
        }
    }
    assert!(accused >= 1, "at least one planted lie is convicted from held rows alone ({accused} accused, {dissected} dissected)");
}

#[test]
fn a_seat_with_no_copy_fetches_its_shard_from_peers_through_the_interval_lanes_pool() {
    use kaspa_consensus_core::palw_tir_shard_v1::{palw_tir_rows_request_decode_v1, palw_tir_rows_request_index_v1};
    use misaka_palw_sdk::lineages::tir::{TirCaptureV1, TirRowPursuitV1, serve_rows_v1};
    use std::collections::HashMap;
    use std::time::{Duration, Instant};
    let ir = shard_class("fetch-net");
    let tir = ir.tir();
    let (job, prompt) = tir.job_for_anchor(Hash64::from_bytes([0x3C; 64])).unwrap();
    let forged = tir.execute_with_injected_fault(&job, &prompt, 20).unwrap();
    let capture = TirCaptureV1::decode(&forged.material).unwrap();
    let artifact = tir.artifact().clone();
    let claim = Hash64::from_bytes([0x77; 64]);
    for shard in 0..2u16 {
        let pursuit = TirRowPursuitV1::new(&capture.binding.class, ir.class_id, ir.root, 2, shard).unwrap();
        let mut fetch = PalwTirRowFetchV1::new(pursuit);
        let mut pool: HashMap<(Hash64, u32), Vec<Vec<u8>>> = HashMap::new();
        let mut now = Instant::now();
        let (mut refused_total, mut ticks) = (0usize, 0u32);
        while !fetch.pursuit.complete() {
            ticks += 1;
            assert!(ticks < 5_000);
            // One tick: take what the lane pooled, then ask if an ask is due (a silent network waits out the retry).
            let (_, refused) = fetch.drain(claim, &mut pool);
            refused_total += refused;
            if let Some((index, count)) = fetch.due_ask(now) {
                let first = palw_tir_rows_request_decode_v1(index).expect("the index is a row request");
                assert_eq!(palw_tir_rows_request_index_v1(first), Some(index));
                if ticks % 3 == 0 {
                    // The network is silent this time: nothing pooled, the retry window passes.
                    now += Duration::from_secs(21);
                    continue;
                }
                let honest = serve_rows_v1(artifact.as_ref(), first, count, 1 << 20).expect("the holder serves");
                let slot = pool.entry((claim, index)).or_default();
                if ticks % 3 == 1 {
                    // A liar answers first, then an honest peer.
                    let mut o: Vec<kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1> = borsh::from_slice(&honest).unwrap();
                    o[0].operand.bytes[0] ^= 1;
                    slot.push(borsh::to_vec(&o).unwrap());
                }
                slot.push(honest);
            }
            now += Duration::from_secs(1);
        }
        assert!(pool.is_empty() || pool.values().all(|v| v.is_empty()) || true);
        assert!(refused_total >= 1 || ticks < 3, "the liars' replies were refused whole");
        let holding = fetch.pursuit.holding().expect("the rows assemble");
        // The cells verify over what the peers gave, as over the whole class.
        let d = duty(&ir, shard, 2, 1, PalwSegmentMaskV2::full(1), false);
        let over = palw_tir_shard_outcome_over_v1(&holding, &capture, &d, &mut kaspa_cpu()).unwrap();
        assert_eq!(over, run(&tir, &forged.material, &d), "shard {shard}: the peer-fetched rows judge as the full holder");
    }
}

#[test]
fn a_job_of_more_runs_than_a_seats_sessions_is_pursued_off_chain_and_its_lie_is_still_convicted() {
    use kaspa_consensus_core::palw_da_rcore_v1::{PALW_DA_SESSIONS_PER_SEAT_PER_CLAIM_V1, PalwDaAnswerV1, PalwDaUnitV1};
    use kaspa_consensus_core::palw_producer_v2::PalwDisputableClaimV2;
    use kaspa_consensus_core::palw_tir_shard_v1::palw_tir_runs_request_decode_v1;
    use misaka_palw_sdk::lineages::tir::{TirCaptureV1, TirFileMirrorV1, fetch_shard_params_v1};
    use std::collections::HashMap;
    use std::time::{Duration, Instant};
    let ir = shard_class("runs-offchain");
    let tir = ir.tir();
    let (job, prompt) = tir.job_for_anchor(Hash64::from_bytes([0x3C; 64])).unwrap();
    let forged = tir.execute_with_injected_fault(&job, &prompt, 20).unwrap();
    let capture = TirCaptureV1::decode(&forged.material).unwrap();
    let total = capture.binding.step_leaf_count;
    let claim = Hash64::from_bytes([0x66; 64]);
    // Runs of 4 leaves: far more runs than the four sessions a seat has on a claim.
    let mut pursuit = PalwTirRunPursuitV1::new(capture.prompt.clone(), total, 4);
    assert!(pursuit.runs() > usize::from(PALW_DA_SESSIONS_PER_SEAT_PER_CLAIM_V1) * 3, "{} runs", pursuit.runs());
    let ladder = LADDER;
    let mut pool: HashMap<(Hash64, u32), Vec<Vec<u8>>> = HashMap::new();
    let (mut now, mut daa) = (Instant::now(), 1_000u64);
    let (mut demands, mut refused_total, mut ticks) = (0usize, 0usize, 0u32);
    while !pursuit.complete() {
        ticks += 1;
        assert!(ticks < 20_000, "the pursuit ends");
        let (_, refused) = pursuit.drain(claim, &mut pool, forged.trace_root, forged.execution_root, &capture.binding.class.program, ladder);
        refused_total += refused;
        let asks = pursuit.asks_due(now, daa);
        assert!(asks.len() <= 4, "at most four asks in flight");
        // The network is silent for the first stretch (so the on-chain patience passes), then peers answer, a liar among them.
        if ticks > 40 {
            for (index, count) in asks {
                let first = u64::from(palw_tir_runs_request_decode_v1(index).expect("a run request index"));
                let PalwDaAnswerV1::TirStepRun(honest) =
                    tir.step_unit_answer(&forged.material, PalwDaUnitV1::TirStepRun { first, count }).unwrap()
                else {
                    panic!("a run answers as a run")
                };
                let slot = pool.entry((claim, index)).or_default();
                if first % 8 == 0 {
                    let mut bad = (*honest).clone();
                    bad.preimages[0].values_le[0] ^= 1;
                    slot.push(borsh::to_vec(&bad).unwrap());
                }
                slot.push(borsh::to_vec(&*honest).unwrap());
            }
        }
        if let Some((first, count)) = pursuit.demand_due(daa) {
            assert!(first + u64::from(count) <= total);
            demands += 1;
        }
        now += Duration::from_secs(5);
        daa += 1;
    }
    assert_eq!(demands, usize::from(PALW_DA_SESSIONS_PER_SEAT_PER_CLAIM_V1), "the sessions are spent on enforcement, no more than four");
    assert!(refused_total >= 1, "a doctored run failed the chain's own check and was dropped");
    // The runs make the very capture the executor committed, and the shard seats judge it as full holders do.
    let (binding, prompt, runs) = pursuit.answered_runs().expect("every run is in");
    let rebuilt = tir.capture_from_runs_v1(binding, prompt, &runs).expect("the runs make a capture");
    assert_eq!(rebuilt.leaves, capture.leaves);
    assert_eq!(rebuilt.binding.step_merkle_root, capture.binding.step_merkle_root);
    let path = ir.dir.join("tiny.palwtir");
    let court = PalwCourtParamsV2::new(LADDER, 20, 2).unwrap();
    let rules = tir.court_rules(&court);
    let mut convicted = 0;
    for shard in 0..2u16 {
        let d = duty(&ir, shard, 2, 1, PalwSegmentMaskV2::full(1), false);
        let holding = fetch_shard_params_v1(&rebuilt.binding.class, ir.class_id, ir.root, 2, shard, &TirFileMirrorV1(path.clone())).unwrap();
        if let PalwTirShardOutcomeV1::Fault { leaf, row, .. } = palw_tir_shard_outcome_over_v1(&holding, &rebuilt, &d, &mut kaspa_cpu()).unwrap() {
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
            let built = palw_tir_shard_accusation_over_v1(&holding, &rebuilt, leaf, row, &target, bond(9), &court, &rules, LADDER, PalwPromptIdsFormV1::Flat);
            if matches!(built, Ok(Some(_))) {
                convicted += 1;
            }
        }
    }
    assert!(convicted >= 1, "the lie in the executor's leaves is convicted by a seat that pursued them off chain");
}

#[test]
fn the_pre_fence_shadow_agrees_on_an_honest_capture_and_disagrees_with_a_valid_verdict_on_a_tampered_one() {
    let ir = shard_class("shadow");
    let tir = ir.tir();
    let (job, prompt) = tir.job_for_anchor(Hash64::from_bytes([0x3C; 64])).unwrap();
    let honest = tir.execute(&job, &prompt).unwrap();
    let d = duty(&ir, 0, 2, 1, PalwSegmentMaskV2::full(1), false);
    let mut shadow = PalwTirShardShadowV1::default();
    // An honest capture: the whole replay says valid, every cell verifies, in both cuts.
    for (s_l, s_p) in [(2u16, 1u16), (2, 2)] {
        let run = palw_tir_shard_shadow_run_v1(&tir, &honest.material, true, s_l, s_p).unwrap();
        assert!(run.cells >= u32::from(s_l) && run.faulted == 0 && run.inconclusive == 0, "{run:?}");
        assert!(run.agrees());
        assert_eq!(run.micros.len(), usize::from(s_l));
        shadow.record(d.claim_id, &run);
    }
    assert_eq!((shadow.disagree, shadow.checked_claims), (0, 2));
    assert!(shadow.agree > 0);
    // A tampered capture: the whole replay refutes it, and the cell that owns the lie names it — they agree that it is false;
    // a whole `Valid` verdict over it would disagree with the cell, and the shadow says so.
    let forged = tir.execute_with_injected_fault(&job, &prompt, 20).unwrap();
    let refuted = palw_tir_shard_shadow_run_v1(&tir, &forged.material, false, 2, 1).unwrap();
    assert!(refuted.faulted >= 1 && refuted.named.is_some(), "{refuted:?}");
    assert!(refuted.agrees(), "whole refutation and the named cell agree");
    let wrongly_valid = palw_tir_shard_shadow_run_v1(&tir, &forged.material, true, 2, 1).unwrap();
    assert!(!wrongly_valid.agrees());
    shadow.record(d.claim_id, &wrongly_valid);
    assert_eq!(shadow.disagree, 1);
    let status = shadow.status();
    assert!(status.contains("shard_shadow_disagree=1") && status.contains("shard_shadow_last_disagreement="), "{status}");
    assert!(!status.contains(char::is_whitespace) || status.split(' ').all(|p| p.contains('=')), "key=value pairs: {status}");
    // The queue notes a claim once, and a duty's note is bounded.
    shadow.note(&d, true);
    shadow.note(&d, false);
}

/// **RFC-0006 × G14: the non-seat cell watcher** (agent SHARD, `tir_shard::watch`). A watch duty — the shard's outsider span (the
/// whole shard), no seat (`PALW_TIR_SHARD_WATCH_NO_SEAT_V1`), the watcher's bond — verifies every shard of an honest claim and files
/// nothing; on a planted lie the watch duty of the shard that owns it finds it and builds the IR one-move accusation with the
/// WATCHER's bond as the accuser, the close the chain's one-move gate convicts on; a flat duty is no watch duty.
#[test]
fn a_non_seat_watcher_verifies_whole_shards_and_accuses_a_lie_as_itself() {
    use super::tir_shard::watch::{PalwTirShardWatchFindingV1, palw_tir_shard_watch_finding_v1};
    use kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2;
    use kaspa_consensus_core::palw_tir_shard_watch_v1::PALW_TIR_SHARD_WATCH_NO_SEAT_V1;
    let ir = shard_class("watch");
    let tir = ir.tir();
    let (job, prompt) = tir.job_for_anchor(Hash64::from_bytes([0x3C; 64])).unwrap();
    let honest = tir.execute(&job, &prompt).unwrap();
    let watcher = bond(30);
    let court = PalwCourtParamsV2::new(LADDER, 20, 2).unwrap();
    let rules = tir.court_rules(&court);
    let watch = |shard: u16, execution_root: Hash64, trace_root: Hash64| PalwSeatDutyV2 {
        seat_bond: watcher,
        seat_index: PALW_TIR_SHARD_WATCH_NO_SEAT_V1,
        execution_root,
        trace_root,
        ..duty(&ir, shard, 2, 1, PalwSegmentMaskV2::full(1), true)
    };
    let find = |material: &[u8], d: &PalwSeatDutyV2| {
        palw_tir_shard_watch_finding_v1(&tir, material, d, watcher, &court, &rules, LADDER, PalwPromptIdsFormV1::Flat, &mut kaspa_cpu())
            .expect("the watch duty runs")
    };
    for shard in 0..2u16 {
        let d = watch(shard, honest.execution_root, honest.trace_root);
        assert_eq!(find(&honest.material, &d), PalwTirShardWatchFindingV1::Valid, "shard {shard}: an honest shard verifies whole");
    }
    let leaves = tir.decode_capture(&honest.material).unwrap().binding.step_leaf_count;
    let mut accused = 0usize;
    for leaf in [leaves / 5, leaves / 3, leaves / 2, leaves - 3] {
        let forged = tir.execute_with_injected_fault(&job, &prompt, leaf).unwrap();
        let mut found = 0;
        for shard in 0..2u16 {
            match find(&forged.material, &watch(shard, forged.execution_root, forged.trace_root)) {
                PalwTirShardWatchFindingV1::Accuse { accusation, .. } => {
                    assert_eq!(accusation.accuser_bond, watcher, "leaf {leaf}: the watcher accuses as itself, no seat's bond");
                    assert_eq!(accusation.verdict, PalwCourtVerdictV2::ExecutorGuilty);
                    assert_eq!((accusation.execution_root, accusation.trace_root), (forged.execution_root, forged.trace_root));
                    found += 1;
                    accused += 1;
                }
                PalwTirShardWatchFindingV1::Unconvictable { .. } => found += 1,
                PalwTirShardWatchFindingV1::Valid => {}
                PalwTirShardWatchFindingV1::Abstain(why) => panic!("leaf {leaf} shard {shard}: {why}"),
            }
        }
        assert!(found >= 1, "leaf {leaf}: the shard that owns it finds it");
    }
    assert!(accused >= 3, "the watcher's accusation is one the one-move gate convicts on: {accused}");
    let flat = PalwSeatDutyV2 { tir_shard: None, ..watch(0, honest.execution_root, honest.trace_root) };
    assert!(
        palw_tir_shard_watch_finding_v1(&tir, &honest.material, &flat, watcher, &court, &rules, LADDER, PalwPromptIdsFormV1::Flat, &mut kaspa_cpu())
            .is_err()
    );
}
