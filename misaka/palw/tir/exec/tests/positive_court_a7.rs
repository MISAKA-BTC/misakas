//! Independent A7 acceptance/attack tests for the pinned private R2 prototype.
#![cfg(feature = "node")]
mod a7_support;
use a7_support::*;
use kaspa_consensus_core::{palw_positive_court_v2::*, palw_tir_court_v1::PalwTirConeRefutationV1};
use std::{io::Cursor, path::Path, process::Command, time::Instant};

#[test]
fn yardstick_context_requires_the_authenticated_class_tokenizer_before_inference() {
    use misaka_palw_tir_exec::{TirParams, TirPlan, node::positive_court::miner_infer_v2};
    let (class, result) = setup(false, false, 1);
    let mut ctx =
        kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, result.job.shape_profile_id, (1, 3)).unwrap();
    ctx.prompt_token_ids_hash = result.job.prompt_token_ids_hash;
    assert_ne!(ctx.tokenizer_id, class.tokenizer_id);
    let plan = TirPlan::compile(&program(false, false)).unwrap();
    let map = params(false);
    let params = TirParams::from_map(&plan, &map).unwrap();
    assert!(
        miner_infer_v2(
            &class,
            result.artifact,
            &plan,
            &params,
            &ctx,
            &[1],
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            limits()
        )
        .is_err()
    );
    ctx.tokenizer_id = class.tokenizer_id;
    let run = miner_infer_v2(
        &class,
        result.artifact,
        &plan,
        &params,
        &ctx,
        &[1],
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        limits(),
    )
    .unwrap();
    assert_eq!(run.generated, [2, 0, 0]);
}

#[test]
fn scalar_oracle_checks_signed_outputs_and_every_logits_lane() {
    assert_eq!(oracle(1, false, false).0, [2, 0, 0]);
    assert_eq!(oracle(2, false, true).0, [0, 0, 0]); // tie at lanes 0/1 selects 0
    for wide in [false, true] {
        for ties in [false, true] {
            for prompt in 0..4 {
                let p = panel(wide, ties, prompt, miner(wide, ties, prompt), false);
                let (expected, rows) = oracle(prompt, wide, ties);
                assert_eq!(p.witness.generated, expected);
                let mut row = 0;
                for (cone, trace) in &p.parts {
                    if trace.is_none() {
                        continue;
                    }
                    let cone: PalwTirConeRefutationV1 = borsh::from_slice(cone).unwrap();
                    let width = if wide { 8 } else { 4 };
                    let actual: Vec<i128> = cone
                        .output_preimage
                        .values_le
                        .chunks_exact(width)
                        .map(|b| {
                            if wide {
                                i64::from_le_bytes(b.try_into().unwrap()) as i128
                            } else {
                                i32::from_le_bytes(b.try_into().unwrap()) as i128
                            }
                        })
                        .collect();
                    assert_eq!(actual, rows[row], "wide={wide} ties={ties} prompt={prompt} row={row}");
                    row += 1;
                }
                assert_eq!(row, 3);
                let mut c = p.admit().unwrap();
                p.submit_all(&mut c).unwrap();
                assert_eq!(c.finish(3), Ok(ClaimStatusV2::Passed));
            }
        }
    }
}

#[test]
fn corruption_at_each_leaf_and_every_proper_prefix_never_passes_or_convicts_miner() {
    for wide in [false, true] {
        let p = fixture(wide);
        for index in 0..p.parts.len() {
            let mut c = p.admit().unwrap();
            for (cone, trace) in &p.parts[..index] {
                c.submit_encoded(cone, trace.as_deref(), 2).unwrap();
            }
            assert_eq!(c.finish(3), Err(ErrorV2::Incomplete));
            let mut bad: PalwTirConeRefutationV1 = borsh::from_slice(&p.parts[index].0).unwrap();
            bad.output_preimage.values_le[0] ^= 1;
            assert!(c.submit_encoded(&borsh::to_vec(&bad).unwrap(), p.parts[index].1.as_deref(), 4).is_err());
            assert_eq!(c.statuses(), (ClaimStatusV2::Checking, WitnessStatusV2::Rejected));
            assert!(c.finish(5).is_err());
            c.expire(100);
            assert_eq!(c.statuses().0, ClaimStatusV2::EndedUnverified);
        }
        for first in 1..p.parts.len() {
            let mut c = p.admit().unwrap();
            assert_eq!(c.submit_encoded(&p.parts[first].0, p.parts[first].1.as_deref(), 2), Err(ErrorV2::Order));
            assert_eq!(c.statuses().0, ClaimStatusV2::Checking);
        }
        let mut c = p.admit().unwrap();
        c.submit_encoded(&p.parts[0].0, p.parts[0].1.as_deref(), 2).unwrap();
        assert_eq!(c.submit_encoded(&p.parts[0].0, p.parts[0].1.as_deref(), 2), Err(ErrorV2::Order));
    }
}

#[test]
fn all_signers_and_self_consistent_wrong_weights_cannot_override_registered_data() {
    for wide in [false, true] {
        let honest = miner(wide, false, 1);
        let mut p = panel(wide, false, 1, honest, true);
        assert_ne!(p.witness.generated, oracle(1, wide, false).0);
        // Miner and Panel both sign the wrong result. Node still uses registered data.
        p.result.output =
            kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1(&p.result.job, &p.witness.generated);
        p.resign();
        let mut c = p.admit().unwrap();
        assert!(p.submit_all(&mut c).is_err());
        assert_eq!(c.statuses(), (ClaimStatusV2::Checking, WitnessStatusV2::Rejected));
        assert!(c.finish(3).is_err());
    }
}

#[test]
fn signed_foreign_scope_input_and_role_substitutions_are_denied() {
    let original = fixture(false);
    for dimension in 0..8 {
        let mut p = original.clone();
        match dimension {
            0 => p.result.network = h(101),
            1 => p.result.branch = h(102),
            2 => p.result.ruleset = h(103),
            3 => p.result.job.prompt_token_ids_hash = h(104),
            4 => p.result.job.tokenizer_id = h(105),
            5 => p.result.job.shape_profile_id = h(106),
            6 => p.class.tokenizer_id = h(107),
            7 => p.result.artifact = h(108),
            _ => unreachable!(),
        }
        p.resign();
        assert_eq!(p.admit().err(), Some(ErrorV2::Binding), "dimension {dimension}");
    }
    let mut p = original.clone();
    p.miner_sig = sign(51, p.result.id(), WITNESS_CONTEXT).1;
    assert_eq!(p.admit().err(), Some(ErrorV2::Authentication));
    let mut p = original.clone();
    p.panel_sig = sign(61, p.witness.id(), RESULT_CONTEXT).1;
    assert_eq!(p.admit().err(), Some(ErrorV2::Authentication));
    let mut p = original;
    std::mem::swap(&mut p.miner_key, &mut p.panel_key);
    assert_eq!(p.admit().err(), Some(ErrorV2::Authentication));
}

#[test]
fn complete_exact_check_is_required_for_refutation_and_never_grants_native_credit() {
    for wide in [false, true] {
        for wrong_output in [false, true] {
            let mut p = fixture(wide);
            if wrong_output {
                p.result.output = h(999);
                p.resign();
            }
            let mut c = p.admit().unwrap();
            for consumer in gate::NATIVE_CREDIT_CONSUMERS_V2 {
                assert_eq!(
                    gate::positive_native_credit_v2(Some(&c), &p.scope(), h(2), h(3), consumer),
                    Err(gate::NativeCreditDenialV2::PositiveNotPassed)
                );
            }
            p.submit_all(&mut c).unwrap();
            // All parts without Finish are still Checking.
            assert_eq!(c.statuses().0, ClaimStatusV2::Checking);
            let expected = if wrong_output { ClaimStatusV2::Refuted } else { ClaimStatusV2::Passed };
            assert_eq!(c.finish(3), Ok(expected));
            for consumer in gate::NATIVE_CREDIT_CONSUMERS_V2 {
                let denial = if wrong_output {
                    gate::NativeCreditDenialV2::PositiveNotPassed
                } else {
                    gate::NativeCreditDenialV2::MissingNativeResultBinding
                };
                assert_eq!(gate::positive_native_credit_v2(Some(&c), &p.scope(), h(2), h(3), consumer), Err(denial));
            }
            c.revoke();
            assert!(c.finish(4).is_err());
        }
    }
}

fn journal_bytes(p: &Packet, count: usize, finish: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    journal::write_header(&p.admit().unwrap(), &mut bytes).unwrap();
    for (cone, trace) in &p.parts[..count] {
        journal::write_part(&mut bytes, cone, trace.as_deref(), p.policy.max_part_bytes).unwrap();
    }
    if finish {
        journal::write_finish(&mut bytes).unwrap();
    }
    bytes
}
#[test]
fn recovery_reauthenticates_all_frames_and_cannot_resurrect_a_pass_or_extend_deadline() {
    let p = fixture(false);
    let full = journal_bytes(&p, p.parts.len(), true);
    let mut c = p.admit().unwrap();
    assert_eq!(journal::replay(&mut c, &mut Cursor::new(&full), 3), Ok(ClaimStatusV2::Passed));
    for cut in 0..p.parts.len() {
        let bytes = journal_bytes(&p, cut, false);
        let mut c = p.admit().unwrap();
        assert_eq!(journal::replay(&mut c, &mut Cursor::new(&bytes), 3), Ok(ClaimStatusV2::Checking));
        assert_eq!(c.cursor(), cut as u64);
        let expected_bytes: u64 = p.parts[..cut].iter().map(|(c, t)| (c.len() + t.as_ref().map_or(0, Vec::len)) as u64).sum();
        assert_eq!(c.charged_bytes(), expected_bytes);
        assert_eq!(c.finish(4), Err(ErrorV2::Incomplete));
        // Crash truncating the payload cannot be interpreted as a complete frame.
        if cut > 0 {
            let mut c = p.admit().unwrap();
            assert!(journal::replay(&mut c, &mut Cursor::new(&bytes[..bytes.len() - 1]), 3).is_err());
            assert_ne!(c.statuses().0, ClaimStatusV2::Passed);
        }
    }
    for offset in [0, 8, 72, 136, full.len() - 2] {
        let mut bad = full.clone();
        bad[offset] ^= 1;
        let mut c = p.admit().unwrap();
        assert!(journal::replay(&mut c, &mut Cursor::new(bad), 3).is_err());
        assert_ne!(c.statuses().0, ClaimStatusV2::Passed);
    }
    let mut trailing = full.clone();
    trailing.push(2);
    let mut c = p.admit().unwrap();
    assert!(journal::replay(&mut c, &mut Cursor::new(trailing), 3).is_err());
    assert_ne!(c.statuses().0, ClaimStatusV2::Passed);
    let mut c = p.admit().unwrap();
    assert_eq!(journal::replay(&mut c, &mut Cursor::new(full), 100), Err(ErrorV2::Deadline));
    assert_eq!(c.statuses().0, ClaimStatusV2::EndedUnverified);
}

#[test]
fn overlapping_source_requests_match_an_independent_set_and_pay_each_chunk_once() {
    use kaspa_consensus_core::palw_step_leg::{step_merkle_root_v1, step_opening_v1};
    use service::*;
    let chunks: Vec<_> = (0..8).map(|i| vec![i; CHUNK_BYTES]).collect();
    let leaves: Vec<_> = chunks.iter().enumerate().map(|(i, b)| chunk_hash_v2(&h(4), i as u64, b)).collect();
    let policy = ServicePolicyV2 {
        version: 2,
        case: h(4),
        source: step_merkle_root_v1(&leaves).unwrap(),
        chunks: 8,
        max_rounds: 3,
        max_unique_bytes: 8 * CHUNK_BYTES as u64,
        max_transmitted_bytes: 1 << 22,
        work_per_chunk: 7,
        max_work: 1 << 20,
        retention_until: 120,
        deadline: 100,
        price_per_chunk: 11,
        hold: 100,
    };
    let mut s = ServiceLedgerV2::new(policy.clone(), policy.id(), 1).unwrap();
    let mut requested = std::collections::BTreeSet::new();
    let mut rounds = 0;
    let mut seed = 0x17a7u64;
    for _ in 0..128 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let first = seed % 8;
        let count = 1 + ((seed >> 8) % (8 - first));
        let fresh = (first..first + count).any(|i| !requested.contains(&i));
        let before = s.meter();
        let got = s.request(WorkRequestV2::Source { first, count }, 2);
        if fresh && rounds == 3 {
            assert_eq!(got, Err(ErrorV2::Budget));
            assert_eq!(s.meter(), before);
        } else {
            assert_eq!(got, Ok(if fresh { RequestEffectV2::Opened } else { RequestEffectV2::Coalesced }));
            if fresh {
                rounds += 1;
            }
            requested.extend(first..first + count);
        }
        assert_eq!(s.meter().rounds, rounds);
        assert_eq!(s.meter().unique_bytes, requested.len() as u64 * CHUNK_BYTES as u64);
        assert!(s.conserved());
    }
    for i in &requested {
        let proof = step_opening_v1(&leaves, *i).unwrap();
        let bytes = &chunks[*i as usize];
        let mut bad = bytes.clone();
        bad[0] ^= 1;
        let before = s.meter();
        assert_eq!(s.respond(*i, &bad, &proof, 3), Err(ErrorV2::Binding));
        assert!(s.meter().transmitted_bytes > before.transmitted_bytes);
        assert_eq!(s.meter().paid, before.paid);
        s.respond(*i, bytes, &proof, 3).unwrap();
        s.respond(*i, bytes, &proof, 3).unwrap();
    }
    assert!(s.data_complete());
    assert_eq!(s.meter().paid, requested.len() as u64 * 11);
    let paid = s.meter().paid;
    s.close();
    let settled = s.meter();
    s.close();
    assert_eq!(s.meter(), settled);
    assert_eq!(settled.refunded, 100 - paid);
    assert!(s.conserved());
    for work in [
        WorkRequestV2::BuildWitness,
        WorkRequestV2::ReconstructTrace,
        WorkRequestV2::ComputeProjection,
        WorkRequestV2::ReplayCheckpoint,
    ] {
        let mut s = ServiceLedgerV2::new(policy.clone(), policy.id(), 1).unwrap();
        let before = s.meter();
        assert_eq!(s.request(work, 2), Err(ErrorV2::UnsupportedProfile));
        assert_eq!(s.meter(), before);
    }
}

fn usage() -> serde_json::Value {
    #[cfg(unix)]
    {
        let mut u = std::mem::MaybeUninit::<libc::rusage>::uninit();
        assert_eq!(unsafe { libc::getrusage(libc::RUSAGE_SELF, u.as_mut_ptr()) }, 0);
        let u = unsafe { u.assume_init() };
        return serde_json::json!({"peak_rss_bytes": u.ru_maxrss as u64 * if cfg!(target_os="macos") {1} else {1024},
            "user_cpu_seconds": u.ru_utime.tv_sec as f64 + u.ru_utime.tv_usec as f64 / 1e6,
            "system_cpu_seconds": u.ru_stime.tv_sec as f64 + u.ru_stime.tv_usec as f64 / 1e6});
    }
    #[cfg(not(unix))]
    {
        serde_json::Value::Null
    }
}
fn read_packet(dir: &Path) -> Packet {
    borsh::from_slice(&std::fs::read(dir.join("packet.borsh")).unwrap()).unwrap()
}

// Spawned with --exact and --ignored. Ordinary test runs cannot accidentally run
// a process worker. Only baseline/Miner/Panel branches construct model params.
#[test]
#[ignore = "invoked in isolated child processes by the A7 process acceptance test"]
fn a7_role_worker() {
    let role = std::env::var("PC_A7_ROLE").unwrap();
    let dir = std::path::PathBuf::from(std::env::var_os("PC_A7_DIR").unwrap());
    let start = Instant::now();
    let detail = match role.as_str() {
        "baseline" => {
            use misaka_palw_tir_exec::{NoSink, Slice, TirExecutor, TirParams, TirPlan};
            let plan = TirPlan::compile(&program(false, false)).unwrap();
            let map = params(false);
            let params = TirParams::from_map(&plan, &map).unwrap();
            let t = Instant::now();
            let mut e = TirExecutor::new(&plan, &params).unwrap();
            let mut ids = Vec::new();
            let mut token = 1;
            for _ in 0..3 {
                e.step_opt(token, &mut NoSink, true).unwrap();
                let Slice::I32(row) = e.logits().1 else { unreachable!() };
                token = (1..row.len()).fold(0, |best, i| if row[i] > row[best] { i } else { best }) as u32;
                ids.push(token);
            }
            assert_eq!(ids, oracle(1, false, false).0);
            serde_json::json!({"inference_seconds":t.elapsed().as_secs_f64(), "generated":ids})
        }
        "miner" => {
            let result = miner(false, false, 1);
            let (key, sig) = sign(51, result.id(), RESULT_CONTEXT);
            std::fs::write(dir.join("result.borsh"), borsh::to_vec(&(result, key, sig)).unwrap()).unwrap();
            serde_json::json!({"result_commit_bytes": std::fs::metadata(dir.join("result.borsh")).unwrap().len(),
                "note":"includes private artifact-index/setup cost; no Panel capture or witness builder in this branch"})
        }
        "panel" => {
            let (result, key, sig): (MinerResultV2, Vec<u8>, Vec<u8>) =
                borsh::from_slice(&std::fs::read(dir.join("result.borsh")).unwrap()).unwrap();
            let mut p = panel(false, false, 1, result, false);
            assert_eq!(p.miner_key, key);
            assert_eq!(p.miner_sig, sig);
            p.miner_key = key;
            p.miner_sig = sig;
            let count = p.parts.len();
            let bytes: usize = p.parts.iter().map(|(c, t)| c.len() + t.as_ref().map_or(0, Vec::len)).sum();
            std::fs::write(dir.join("packet.borsh"), borsh::to_vec(&p).unwrap()).unwrap();
            serde_json::json!({"leaves":count,"evidence_bytes":bytes,
                "artifact_root":p.result.artifact.to_string(),"class_id":p.result.job.shape_profile_id.to_string(),
                "input_root":p.result.job.prompt_token_ids_hash.to_string(),"output_root":p.result.output.to_string(),
                "policy_id":p.policy.id().to_string(),"ruleset":p.result.ruleset.to_string()})
        }
        "node" | "prefix" | "restart" | "node-refute" | "node-reject" => {
            let p = read_packet(&dir);
            let mut c = p.admit().unwrap();
            if role == "node-reject" {
                assert!(p.submit_all(&mut c).is_err());
                assert_eq!(c.statuses(), (ClaimStatusV2::Checking, WitnessStatusV2::Rejected));
                assert!(c.finish(3).is_err());
            } else if role == "node-refute" {
                p.submit_all(&mut c).unwrap();
                assert_eq!(c.statuses().0, ClaimStatusV2::Checking);
                assert_eq!(c.finish(3), Ok(ClaimStatusV2::Refuted));
            } else if role == "node" {
                p.submit_all(&mut c).unwrap();
                assert_eq!(c.finish(3), Ok(ClaimStatusV2::Passed));
            } else if role == "prefix" {
                let bytes = journal_bytes(&p, p.parts.len() / 2, false);
                assert_eq!(journal::replay(&mut c, &mut Cursor::new(&bytes), 3), Ok(ClaimStatusV2::Checking));
                std::fs::write(dir.join("restart.journal"), bytes).unwrap();
            } else {
                let bytes = std::fs::read(dir.join("restart.journal")).unwrap();
                assert_eq!(journal::replay(&mut c, &mut Cursor::new(bytes), 3), Ok(ClaimStatusV2::Checking));
                let cut = c.cursor() as usize;
                for (cone, trace) in &p.parts[cut..] {
                    c.submit_encoded(cone, trace.as_deref(), 4).unwrap();
                }
                assert_eq!(c.finish(5), Ok(ClaimStatusV2::Passed));
            }
            serde_json::json!({"status":format!("{:?}",c.statuses().0), "cursor":c.cursor(), "charged_bytes":c.charged_bytes(),
                "node_input":"registered class, signed commitments, evidence only", "native_credit":false})
        }
        _ => panic!("unknown role"),
    };
    let report = serde_json::json!({"role":role,"pid":std::process::id(),"elapsed_seconds":start.elapsed().as_secs_f64(),"usage":usage(),"detail":detail});
    std::fs::write(dir.join(format!("{role}.json")), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}

#[test]
fn actual_processes_separate_roles_and_resume_a_checked_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let node_dir = tempfile::tempdir().unwrap();
    let exe = std::env::current_exe().unwrap();
    let mut reports = Vec::new();
    for role in ["baseline", "miner", "panel", "node", "prefix", "restart", "node-refute", "node-reject"] {
        if role == "node" {
            // Separate working directory: Node receives only the packet; not the
            // Miner's file, artifact source, or any executor/model provider.
            std::fs::copy(dir.path().join("packet.borsh"), node_dir.path().join("packet.borsh")).unwrap();
        }
        if role == "node-refute" || role == "node-reject" {
            let mut p = read_packet(dir.path());
            if role == "node-refute" {
                p.result.output = h(999);
                p.resign();
            } else {
                let mut cone: PalwTirConeRefutationV1 = borsh::from_slice(&p.parts[0].0).unwrap();
                cone.output_preimage.values_le[0] ^= 1;
                p.parts[0].0 = borsh::to_vec(&cone).unwrap();
            }
            std::fs::write(node_dir.path().join("packet.borsh"), borsh::to_vec(&p).unwrap()).unwrap();
        }
        let path = if role.starts_with("node") || ["prefix", "restart"].contains(&role) { node_dir.path() } else { dir.path() };
        let output = Command::new(&exe)
            .args(["--ignored", "--exact", "a7_role_worker", "--nocapture"])
            .env("PC_A7_ROLE", role)
            .env("PC_A7_DIR", path)
            .current_dir(path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{role}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&std::fs::read(path.join(format!("{role}.json"))).unwrap()).unwrap();
        assert_ne!(report["pid"].as_u64().unwrap(), std::process::id() as u64);
        reports.push(report);
    }
    assert_eq!(reports[3]["detail"]["status"], "Passed");
    assert_eq!(reports[4]["detail"]["status"], "Checking");
    assert_eq!(reports[5]["detail"]["status"], "Passed");
    assert_eq!(reports[6]["detail"]["status"], "Refuted");
    assert_eq!(reports[7]["detail"]["status"], "Checking");
    assert_eq!(reports[3]["detail"]["charged_bytes"], reports[5]["detail"]["charged_bytes"]);
    let result = serde_json::json!({"schema":"positive-court-a7-toy-process-v1", "profile":"4x4-embedding-3-decode-i32",
        "activation_allowed":false,"native_network_e2e":false,"real_llm_measurement":false,
        "source_case":"warm Panel; no Miner requests", "reports":reports});
    if let Some(path) = std::env::var_os("PC_A7_REPORT_DIR") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path.join("process-report.json")).unwrap();
        std::io::Write::write_all(&mut file, &serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    }
    println!("A7_PROCESS_REPORT={result}");
}
