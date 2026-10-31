mod support;
use kaspa_consensus_core::palw_tir_court_v1::PalwTirConeRefutationV1;
use misaka_palw_positive_contract::{identity::*, messages::ClaimForm, ports::NodeChecker, tir, wire::EvidenceFrame};
use misaka_palw_tir_exec::node::{positive_check::CheckOutcome, positive_contract::TirNodeChecker};
use std::{path::Path, process::Command, time::Instant};
use support::*;

#[test]
fn scalar_oracle_checks_every_logit_and_output_without_tir_evaluation() {
    assert_eq!(oracle(false, 1).0, [2, 0, 0]);
    let (tied_ids, tied_rows) = oracle(false, 2);
    assert_eq!(tied_rows[0][0], tied_rows[0][2]);
    assert_eq!(tied_ids[0], 0, "winning ties must choose the lowest index");
    for prompt in 0..4 {
        let (t, p) = fixture(false, prompt, 7);
        let (ids, rows) = oracle(false, prompt);
        assert_eq!(p.generated, ids);
        let mut row = 0;
        for bytes in &p.frames {
            let f = EvidenceFrame::decode(bytes, limits().max_part_bytes).unwrap();
            let part = tir::decode_frame(&f, limits().max_part_bytes).unwrap();
            if part.trace().is_some() {
                let cone: PalwTirConeRefutationV1 = borsh::from_slice(part.cone()).unwrap();
                let values: Vec<i128> = cone
                    .output_preimage
                    .values_le
                    .chunks_exact(4)
                    .map(|b| i32::from_le_bytes(b.try_into().unwrap()) as i128)
                    .collect();
                assert_eq!(values, rows[row]);
                row += 1;
            }
        }
        assert_eq!(row, 3);
        assert_eq!(submit(&p, &t).unwrap().outcome(), CheckOutcome::Matched);
    }
}

#[test]
fn coherent_signed_model_artifact_input_job_and_witness_replacements_cannot_select_expected_state() {
    let (trusted, original) = fixture(false, 1, 7);
    for (model, prompt, job) in [(true, 1, 7), (false, 2, 7), (false, 1, 99), (true, 2, 99)] {
        let (own, foreign) = fixture(model, prompt, job);
        // Each attack packet is valid and fully checked under its OWN metadata;
        // failure under the independently fixed registration is not bad framing.
        assert_eq!(submit(&foreign, &own).unwrap().outcome(), CheckOutcome::Matched);
        foreign
            .result
            .authenticate(foreign.miner_key.as_slice().try_into().unwrap(), foreign.miner_sig.as_slice().try_into().unwrap())
            .unwrap();
        foreign
            .witness
            .authenticate(
                &foreign.result,
                foreign.panel_key.as_slice().try_into().unwrap(),
                foreign.panel_sig.as_slice().try_into().unwrap(),
            )
            .unwrap();
        assert!(foreign.authenticate(&trusted).is_err(), "model={model} prompt={prompt} job={job}");
        assert_eq!(trusted.admission().expected_input, original.result.input);
        assert_eq!(trusted.admission().registered_class, original.result.registered_class);
    }
    assert!(trusted.admission().check(ClaimForm::LegacyTrace, &original.result).is_err());
}

#[test]
fn all_signers_agreeing_on_wrong_arithmetic_still_get_witness_rejection() {
    let (t, _) = fixture(false, 1, 7);
    let r = result(&t, false, 1);
    let mut p = panel(&t, r, false, 1, true);
    assert_ne!(p.generated, oracle(false, 1).0);
    p.result.output = p.witness.candidate_output;
    p.resign_and_reframe();
    let ctx = p.authenticate(&t).unwrap();
    let mut progress = ctx.start().unwrap();
    let checker = TirNodeChecker;
    assert!(
        p.frames.iter().try_for_each(|b| checker.check_authenticated_part(&ctx, &mut progress, p.authenticated(b).unwrap())).is_err()
    );
    assert_eq!(progress.snapshot().outcome, CheckOutcome::WitnessRejected);
    assert!(checker.finish(&ctx, progress).is_err());
}

#[test]
fn each_leaf_corruption_and_every_incomplete_prefix_cannot_finish() {
    let (t, p) = fixture(false, 1, 7);
    let checker = TirNodeChecker;
    for i in 0..p.frames.len() {
        let ctx = p.authenticate(&t).unwrap();
        let mut progress = ctx.start().unwrap();
        for b in &p.frames[..i] {
            checker.check_authenticated_part(&ctx, &mut progress, p.authenticated(b).unwrap()).unwrap();
        }
        let f = EvidenceFrame::decode(&p.frames[i], limits().max_part_bytes).unwrap();
        let part = tir::decode_frame(&f, limits().max_part_bytes).unwrap();
        let mut cone: PalwTirConeRefutationV1 = borsh::from_slice(part.cone()).unwrap();
        cone.output_preimage.values_le[0] ^= 1;
        let cone = borsh::to_vec(&cone).unwrap();
        let payload = tir::ExactPart::new(&cone, part.trace(), limits().max_part_bytes - 284).unwrap().encode();
        let bad = frame(&p.result, &p.witness, i as u64, &payload);
        let mut authored = p.clone();
        authored.frames[i] = bad;
        authored.resign_frame(i);
        assert!(checker.check_authenticated_part(&ctx, &mut progress, authored.authenticated(&authored.frames[i]).unwrap()).is_err());
        assert_eq!(progress.snapshot().outcome, CheckOutcome::WitnessRejected);
        assert!(checker.finish(&ctx, progress).is_err());
        let ctx = p.authenticate(&t).unwrap();
        let mut progress = ctx.start().unwrap();
        for b in &p.frames[..i] {
            checker.check_authenticated_part(&ctx, &mut progress, p.authenticated(b).unwrap()).unwrap();
        }
        assert!(checker.finish(&ctx, progress).is_err());
    }
}

#[test]
fn complete_coverage_is_required_for_output_mismatch_and_outer_bytes_are_charged() {
    let (t, mut p) = fixture(false, 1, 7);
    p.result.output = d(h(999));
    p.resign_and_reframe();
    let terminal = submit(&p, &t).unwrap();
    assert_eq!(terminal.outcome(), CheckOutcome::OutputMismatch);
    let (t, p) = fixture(false, 1, 7);
    let ctx = p.authenticate(&t).unwrap();
    let mut progress = ctx.start().unwrap();
    let checker = TirNodeChecker;
    for b in &p.frames {
        checker.check_authenticated_part(&ctx, &mut progress, p.authenticated(b).unwrap()).unwrap();
    }
    assert_eq!(progress.charged_wire_bytes(), p.frames.iter().map(|b| b.len() as u64).sum::<u64>());
    assert_eq!(progress.charged_signature_bytes(), p.frames.len() as u64 * SIGNATURE_BYTES as u64);
    assert_eq!(
        progress.charged_transport_bytes().unwrap(),
        p.frames.iter().map(|b| b.len() as u64 + SIGNATURE_BYTES as u64).sum::<u64>()
    );
    assert!(misaka_palw_positive_contract::require_native_activation().is_err());
}

#[test]
fn malformed_actor_credential_lengths_are_refusals_not_node_panics() {
    let (trusted, packet) = fixture(false, 1, 7);
    for actor_field in 0..4 {
        let expected = if actor_field % 2 == 0 { PUBKEY_BYTES } else { SIGNATURE_BYTES };
        for length in [0, expected - 1, expected + 1] {
            let mut malformed = packet.clone();
            let field = match actor_field {
                0 => &mut malformed.miner_key,
                1 => &mut malformed.miner_sig,
                2 => &mut malformed.panel_key,
                _ => &mut malformed.panel_sig,
            };
            field.resize(length, 0);
            assert!(malformed.authenticate(&trusted).is_err(), "credential={actor_field} length={length}");
        }
    }
    assert_eq!(submit(&packet, &trusted).unwrap().outcome(), CheckOutcome::Matched);
}

#[test]
fn unsigned_frame_allowance_cannot_authorize_signature_bytes_or_overflow_a_signed_quote() {
    let (trusted, packet) = fixture(false, 1, 7);
    let context = packet.authenticate(&trusted).unwrap();
    let frame = EvidenceFrame::decode(&packet.frames[0], limits().max_part_bytes).unwrap();
    let signed_bytes = frame.signed_wire_bytes();
    let key = packet.panel_key.as_slice().try_into().unwrap();
    let signature = packet.frame_signatures[0].as_slice().try_into().unwrap();
    for insufficient in [frame.wire_bytes(), signed_bytes - 1] {
        assert!(frame.authenticate(&packet.witness, key, signature, insufficient).is_err());
    }
    let authenticated = frame.authenticate(&packet.witness, key, signature, signed_bytes).unwrap();
    let quote = context.signed_transport_quote().unwrap();
    assert_eq!(quote.max_packet_bytes, limits().max_part_bytes + SIGNATURE_BYTES as u64);
    assert_eq!(quote.signature_bytes, packet.frames.len() as u64 * SIGNATURE_BYTES as u64);
    assert_eq!(quote.total_packet_bytes, quote.unsigned_frame_bytes + quote.signature_bytes);
    let mut progress = context.start().unwrap();
    TirNodeChecker.check_authenticated_part(&context, &mut progress, authenticated).unwrap();
    assert_eq!(progress.charged_signature_bytes(), SIGNATURE_BYTES as u64);
    assert_eq!(progress.charged_transport_bytes().unwrap(), signed_bytes);

    // The unsigned reservation fits u64, but the same allowance plus every
    // signature does not. This quote-only attack allocates no giant frame.
    let mut oversized = limits();
    oversized.max_total_bytes = u64::MAX;
    oversized.max_part_bytes = u64::MAX / packet.frames.len() as u64;
    let context = packet.authenticate_limits(&trusted, oversized).unwrap();
    assert!(context.signed_transport_quote().is_err());
    let mut progress = context.start().unwrap();
    assert!(
        TirNodeChecker.check_authenticated_part(&context, &mut progress, packet.authenticated(&packet.frames[0]).unwrap()).is_err()
    );
    assert_eq!(progress.snapshot().outcome, CheckOutcome::Checking);
    assert_eq!(progress.charged_transport_bytes().unwrap(), 0);
}

#[test]
fn authenticated_recovery_prefix_cannot_switch_scope_skip_or_duplicate_frames() {
    let (trusted, packet) = fixture(false, 1, 7);
    let (foreign_trusted, foreign_packet) = fixture(false, 2, 99);
    let foreign_context = foreign_packet.authenticate(&foreign_trusted).unwrap();
    let checker = TirNodeChecker;
    for cut in [1, packet.frames.len() / 2] {
        let context = packet.authenticate(&trusted).unwrap();
        let mut restored = context.start().unwrap();
        // Replay authenticated bytes rather than accepting a stored cursor or
        // completion flag. This is a checker test, not A6 atomic persistence.
        for bytes in &packet.frames[..cut] {
            checker.check_authenticated_part(&context, &mut restored, packet.authenticated(bytes).unwrap()).unwrap();
        }
        let before = restored.snapshot();
        let transport_before = restored.charged_transport_bytes().unwrap();
        // The foreign frame is genuinely author-authenticated, so the failure
        // must come from its incompatible context, not a broken signature.
        let foreign = foreign_packet.authenticated(&foreign_packet.frames[cut]).unwrap();
        assert!(checker.check_authenticated_part(&foreign_context, &mut restored, foreign).is_err());
        assert_eq!(restored.snapshot(), before);
        assert_eq!(restored.charged_transport_bytes().unwrap(), transport_before);
        for index in [cut - 1, cut + 1] {
            assert!(
                checker
                    .check_authenticated_part(&context, &mut restored, packet.authenticated(&packet.frames[index]).unwrap())
                    .is_err()
            );
            assert_eq!(restored.snapshot(), before);
            assert_eq!(restored.charged_transport_bytes().unwrap(), transport_before);
        }
        for bytes in &packet.frames[cut..] {
            checker.check_authenticated_part(&context, &mut restored, packet.authenticated(bytes).unwrap()).unwrap();
        }
        assert_eq!(restored.charged_signature_bytes(), packet.frames.len() as u64 * SIGNATURE_BYTES as u64);
        assert_eq!(checker.finish(&context, restored).unwrap().outcome(), CheckOutcome::Matched);
    }
}

#[test]
fn signature_domains_malformed_frames_and_stale_ordinals_fail() {
    let (t, p) = fixture(false, 1, 7);
    let mut bad = p.clone();
    bad.miner_sig = sign(51, bad.result.id().0, WITNESS_SIGN_CONTEXT).1;
    assert!(bad.authenticate(&t).is_err());
    let mut bad = p.clone();
    bad.panel_sig = sign(61, bad.witness.id().0, RESULT_SIGN_CONTEXT).1;
    assert!(bad.authenticate(&t).is_err());
    for n in [0, 1, 8, 100, p.frames[0].len() - 1] {
        assert!(EvidenceFrame::decode(&p.frames[0][..n], limits().max_part_bytes).is_err());
    }
    let mut extra = p.frames[0].clone();
    extra.push(0);
    assert!(EvidenceFrame::decode(&extra, limits().max_part_bytes).is_err());
    let ctx = p.authenticate(&t).unwrap();
    let mut progress = ctx.start().unwrap();
    let checker = TirNodeChecker;
    assert!(checker.check_authenticated_part(&ctx, &mut progress, p.authenticated(&p.frames[1]).unwrap()).is_err());
    assert_eq!(progress.snapshot().next_leaf, 0);
}

#[test]
fn an_unsigned_relay_tamper_cannot_reject_the_honest_panel_candidate() {
    let (t, p) = fixture(false, 1, 7);
    let ctx = p.authenticate(&t).unwrap();
    let mut progress = ctx.start().unwrap();
    let original = EvidenceFrame::decode(&p.frames[0], limits().max_part_bytes).unwrap();
    let part = tir::decode_frame(&original, limits().max_part_bytes).unwrap();
    let mut cone: PalwTirConeRefutationV1 = borsh::from_slice(part.cone()).unwrap();
    cone.output_preimage.values_le[0] ^= 1;
    let cone = borsh::to_vec(&cone).unwrap();
    let payload = tir::ExactPart::new(&cone, part.trace(), limits().max_part_bytes - 284).unwrap().encode();
    let tampered = frame(&p.result, &p.witness, 0, &payload);
    // Delivery fails before the checker is invoked. Neither Panel nor Miner is
    // assigned a computation fault by a third party's unauthenticated bytes.
    assert!(p.authenticated(&tampered).is_err());
    assert_eq!(progress.snapshot().outcome, CheckOutcome::Checking);
    assert_eq!(progress.snapshot().next_leaf, 0);
    assert_eq!(progress.charged_wire_bytes(), 0);
    // The exact same author-signed packet can still be relayed and checked.
    for b in &p.frames {
        TirNodeChecker.check_authenticated_part(&ctx, &mut progress, p.authenticated(b).unwrap()).unwrap();
    }
    assert_eq!(TirNodeChecker.finish(&ctx, progress).unwrap().outcome(), CheckOutcome::Matched);
    let mut wrong_domain = p.clone();
    wrong_domain.frame_signatures[0] = sign(61, original.id().0, RESULT_SIGN_CONTEXT).1;
    assert!(wrong_domain.authenticated(&wrong_domain.frames[0]).is_err());
}

#[test]
fn exhausted_evaluation_budget_is_unverified_and_never_an_arithmetic_conviction() {
    let (t, p) = fixture(false, 1, 7);
    let mut cap = limits();
    cap.max_elements_per_leaf = 1;
    cap.max_terms_per_leaf = 1;
    let ctx = p.authenticate_limits(&t, cap).unwrap();
    let mut progress = ctx.start().unwrap();
    assert!(TirNodeChecker.check_authenticated_part(&ctx, &mut progress, p.authenticated(&p.frames[0]).unwrap()).is_err());
    assert_eq!(progress.snapshot().outcome, CheckOutcome::EndedUnverified);
    assert!(TirNodeChecker.finish(&ctx, progress).is_err());
}

fn read<T: borsh::BorshDeserialize>(p: &Path) -> T {
    assert!(std::fs::metadata(p).unwrap().len() < 16 << 20);
    borsh::from_slice(&std::fs::read(p).unwrap()).unwrap()
}
fn write<T: borsh::BorshSerialize>(p: &Path, x: &T) {
    std::fs::write(p, borsh::to_vec(x).unwrap()).unwrap();
}
fn usage() -> serde_json::Value {
    let mut u = std::mem::MaybeUninit::<libc::rusage>::uninit();
    assert_eq!(unsafe { libc::getrusage(libc::RUSAGE_SELF, u.as_mut_ptr()) }, 0);
    let u = unsafe { u.assume_init() };
    serde_json::json!({"peak_rss_bytes":u.ru_maxrss as u64*if cfg!(target_os="macos"){1}else{1024},"user_cpu_seconds":u.ru_utime.tv_sec as f64+u.ru_utime.tv_usec as f64/1e6,"system_cpu_seconds":u.ru_stime.tv_sec as f64+u.ru_stime.tv_usec as f64/1e6})
}

#[test]
#[ignore = "explicit subprocess worker; exercised by process acceptance"]
fn role_worker() {
    let role = std::env::var("PC_A7_ROLE").unwrap();
    let dir = std::path::PathBuf::from(std::env::var_os("PC_A7_DIR").unwrap());
    // This file is produced by the trusted initializer, outside candidate data.
    let trusted: Trusted = read(&dir.join("trusted.borsh"));
    let start = Instant::now();
    let detail = match role.as_str() {
        "baseline" => {
            let t = Instant::now();
            let ids = infer(&trusted, false, 1);
            serde_json::json!({"generated":ids,"inference_with_fixture_setup_seconds":t.elapsed().as_secs_f64()})
        }
        "miner" => {
            let r = result(&trusted, false, 1);
            let (k, s) = sign(51, r.id().0, RESULT_SIGN_CONTEXT);
            write(&dir.join("result.borsh"), &(r, k, s));
            serde_json::json!({"result_bytes":std::fs::metadata(dir.join("result.borsh")).unwrap().len()})
        }
        "panel" => {
            let (r, k, s): (misaka_palw_positive_contract::messages::MinerResultCommit, Vec<u8>, Vec<u8>) =
                read(&dir.join("result.borsh"));
            r.authenticate(k.as_slice().try_into().unwrap(), s.as_slice().try_into().unwrap()).unwrap();
            let p = panel(&trusted, r, false, 1, false);
            write(&dir.join("packet.borsh"), &p);
            serde_json::json!({"leaves":p.frames.len(),"full_frame_bytes":p.frames.iter().map(Vec::len).sum::<usize>(),"signed_delivery_bytes":p.frames.iter().map(Vec::len).sum::<usize>()+p.frame_signatures.iter().map(Vec::len).sum::<usize>(),"artifact":format!("{:?}",trusted.artifact),"input":format!("{:?}",trusted.job.prompt_token_ids_hash)})
        }
        "node" | "prefix" | "restart" | "refute" | "reject" | "foreign" => {
            let p: Packet = read(&dir.join("packet.borsh"));
            if role == "foreign" {
                assert!(p.authenticate(&trusted).is_err());
                serde_json::json!({"status":"RejectedForeignScope"})
            } else {
                let ctx = p.authenticate(&trusted).unwrap();
                let mut progress = ctx.start().unwrap();
                let checker = TirNodeChecker;
                let frames = if role == "prefix" {
                    p.frames[..p.frames.len() / 2].to_vec()
                } else if role == "restart" {
                    read::<Vec<Vec<u8>>>(&dir.join("prefix.borsh"))
                } else {
                    p.frames.clone()
                };
                let mut failed = false;
                for b in &frames {
                    if checker.check_authenticated_part(&ctx, &mut progress, p.authenticated(b).unwrap()).is_err() {
                        failed = true;
                        break;
                    }
                }
                if role == "restart" {
                    let cut = progress.snapshot().next_leaf as usize;
                    for b in &p.frames[cut..] {
                        checker.check_authenticated_part(&ctx, &mut progress, p.authenticated(b).unwrap()).unwrap();
                    }
                }
                let bytes = progress.charged_wire_bytes();
                let signatures = progress.charged_signature_bytes();
                let signed_transport = progress.charged_transport_bytes().unwrap();
                let cursor = progress.snapshot().next_leaf;
                let status = if role == "prefix" {
                    assert!(!failed);
                    write(&dir.join("prefix.borsh"), &frames);
                    assert!(checker.finish(&ctx, progress).is_err());
                    "Checking".into()
                } else if role == "reject" {
                    assert!(failed);
                    assert_eq!(progress.snapshot().outcome, CheckOutcome::WitnessRejected);
                    assert!(checker.finish(&ctx, progress).is_err());
                    "WitnessRejected".into()
                } else {
                    assert!(!failed);
                    let outcome = checker.finish(&ctx, progress).unwrap().outcome();
                    assert_eq!(outcome, if role == "refute" { CheckOutcome::OutputMismatch } else { CheckOutcome::Matched });
                    format!("{outcome:?}")
                };
                serde_json::json!({"status":status,"cursor":cursor,"charged_full_frame_bytes":bytes,
                    "charged_signature_bytes":signatures,"charged_signed_transport_bytes":signed_transport,"native_credit":false})
            }
        }
        _ => panic!("unknown role"),
    };
    let r = serde_json::json!({"role":role,"pid":std::process::id(),"elapsed_seconds":start.elapsed().as_secs_f64(),"usage":usage(),"detail":detail});
    std::fs::write(dir.join(format!("{role}.json")), serde_json::to_vec_pretty(&r).unwrap()).unwrap();
}

#[test]
fn separate_processes_recover_checked_frames_and_reject_a_whole_foreign_packet() {
    let source = tempfile::tempdir().unwrap();
    let node = tempfile::tempdir().unwrap();
    let trusted = registration(false, 1, 7);
    write(&source.path().join("trusted.borsh"), &trusted);
    write(&node.path().join("trusted.borsh"), &trusted);
    let exe = std::env::current_exe().unwrap();
    let mut reports = Vec::new();
    for role in ["baseline", "miner", "panel", "node", "prefix", "restart", "refute", "reject", "foreign"] {
        let is_node = ["node", "prefix", "restart", "refute", "reject", "foreign"].contains(&role);
        let dir = if is_node { node.path() } else { source.path() };
        if role == "node" {
            std::fs::copy(source.path().join("packet.borsh"), node.path().join("packet.borsh")).unwrap();
        }
        if ["refute", "reject", "foreign"].contains(&role) {
            let mut p: Packet = read(&source.path().join("packet.borsh"));
            if role == "refute" {
                p.result.output = d(h(999));
                p.resign_and_reframe();
            } else if role == "foreign" {
                p = fixture(true, 2, 99).1;
            } else {
                let f = EvidenceFrame::decode(&p.frames[0], limits().max_part_bytes).unwrap();
                let part = tir::decode_frame(&f, limits().max_part_bytes).unwrap();
                let mut cone: PalwTirConeRefutationV1 = borsh::from_slice(part.cone()).unwrap();
                cone.output_preimage.values_le[0] ^= 1;
                let cone = borsh::to_vec(&cone).unwrap();
                let payload = tir::ExactPart::new(&cone, part.trace(), limits().max_part_bytes - 284).unwrap().encode();
                p.frames[0] = frame(&p.result, &p.witness, 0, &payload);
                p.resign_frame(0);
            }
            write(&node.path().join("packet.borsh"), &p);
        }
        let output = Command::new(&exe)
            .args(["--ignored", "--exact", "role_worker", "--nocapture"])
            .env("PC_A7_ROLE", role)
            .env("PC_A7_DIR", dir)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{role}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join(format!("{role}.json"))).unwrap()).unwrap();
        reports.push(report);
    }
    assert_eq!(reports[3]["detail"]["charged_full_frame_bytes"], reports[5]["detail"]["charged_full_frame_bytes"]);
    assert_eq!(reports[3]["detail"]["charged_signed_transport_bytes"], reports[5]["detail"]["charged_signed_transport_bytes"]);
    assert_eq!(reports[2]["detail"]["signed_delivery_bytes"], reports[3]["detail"]["charged_signed_transport_bytes"]);
    assert_eq!(reports[8]["detail"]["status"], "RejectedForeignScope");
    let report = serde_json::json!({"schema":"a7-r1-all-node-i32-v1","profile":"warm-4x4-3-decode","reports":reports,
        "a2_miner_adapter_connected":false,
        "activation_allowed":false,"real_llm":false,"cold_model":false,"native_network":false,"a3_builder_port_connected":true,
        "recovery":"test-frame replay only; not A6 atomic/native persistence"});
    if let Some(dir) = std::env::var_os("PC_A7_REPORT_DIR") {
        let output = std::path::PathBuf::from(dir);
        // Raw signed candidates and the separately fixed expected metadata are
        // retained so A0 can inspect/replay the complete replacement attack.
        for (source, name) in [
            (source.path().join("trusted.borsh"), "trusted.borsh"),
            (source.path().join("packet.borsh"), "honest-packet.borsh"),
            (node.path().join("packet.borsh"), "foreign-packet.borsh"),
            (node.path().join("prefix.borsh"), "prefix.borsh"),
        ] {
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(output.join(name)).unwrap();
            std::io::Write::write_all(&mut file, &std::fs::read(source).unwrap()).unwrap();
        }
        assert_eq!(
            std::fs::read(source.path().join("trusted.borsh")).unwrap(),
            std::fs::read(node.path().join("trusted.borsh")).unwrap()
        );
        let path = output.join("process-report.json");
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(path).unwrap();
        std::io::Write::write_all(&mut f, &serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    println!("A7_PROCESS_REPORT={report}");
}
