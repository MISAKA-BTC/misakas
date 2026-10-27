//! **lane V-FUZZ follow-up (coordinator 2026-10-28): why the honest Panel=1 claim does not commit with the 17,400 list armed.**
//! Bisects the list (prefixes armed at DAA 0 over the J1 world, test-only mirrors restored) and, for a failing prefix, replays the
//! reveal against the tip ledger to read the ledger's own refusal.

use super::cert::*;
use super::*;
use futures_util::FutureExt;
use kaspa_consensus_core::config::params::{PALW_T12_G14_17000_FENCES_V1, palw_t12_arm_after_int13_at_v1};
use kaspa_consensus_core::palw_rfc0018_v1::PalwRfc0018MirrorV1;
use misaka_palw_sdk::kernel_job_v1::{kernel_job_from_text_v1, kernel_tokenizer_v1, tests::tiny_tokenizer_json};
use std::panic::AssertUnwindSafe;

fn panic_text(e: Box<dyn std::any::Any + Send>) -> String {
    e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "?".into())
}

/// The J1 world with the first `k` entries of the 17,400 list armed at `at`, the harness's TEST-ONLY mirrors restored.
fn prefix_config(k: usize, at: u64) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, _, premine, floats) = cert_panel_one_config();
    let mut params = config.params.clone();
    palw_t12_arm_after_int13_at_v1(&mut params, &PALW_T12_G14_17000_FENCES_V1[..k], Some(at));
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &mut params.palw_consensus_mode else {
        unreachable!()
    };
    let mirror =
        PalwRfc0018MirrorV1 { certification: TEST_ONLY_KERNEL_CERT_POLICY_V1, ..*b.state.rfc0018_v1().expect("Gate C mirrored") };
    b.state = b
        .state
        .clone()
        .with_test_only_kernel_panel_one_external_gain_bound_v1(Some(super::f03k::TEST_ONLY_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI))
        .with_rfc0018_v1(Some(mirror));
    let bundle = b.clone();
    (Config::new(params), bundle, premine, floats)
}

/// Post a job and claim it; `Ok` when the claim commits, otherwise what was seen (seal row, the ledger's replayed refusal).
async fn probe_claim(parts: (Config, PalwConsensusParamsV2, Premine, Premine)) -> Result<(), String> {
    let mut w = World::on(Net::over_cfg(parts, TestConsensus::new)).await;
    let tok = kernel_tokenizer_v1(&tiny_tokenizer_json(), None).unwrap();
    let job = kernel_job_from_text_v1(
        &tok.tokenizer,
        "hi a",
        Hash64::from_bytes(w.class),
        64,
        MAX_POSITIONS,
        3,
        Hash64::from_bytes([0x71u8; 64]),
    )
    .map_err(|e| format!("job: {e:?}"))?;
    let o = w.net.route(1, &K::PostJob { job: job.clone() });
    w.net.send(vec![(1, o)]).await;
    if !w.net.ledger().jobs.contains_key(&job.id()) {
        return Err(format!("the job did not post (daa {})", w.net.daa()));
    }
    let producer = 0usize;
    let ledger = w.net.ledger();
    let generated = greedy(&w.fx, &ledger, &w.class, &job.prompt, job.max_new_tokens as usize);
    let kid = w.net.kid(producer);
    let produced = produce(&w.fx, &ledger, &w.class, &job, kid, generated, |_| {});
    let (seal, reveal) = seal_and_reveal(&ledger, kid, &produced.object);
    let salted = matches!(reveal, K::CommitClaimSalted { .. });
    let o = w.net.route(producer, &seal);
    w.net.send(vec![(producer, o)]).await;
    let after_seal = w.net.ledger();
    let sealed = after_seal.seals.contains_key(&(job.id(), kid));
    let mut replay = KernelLedgerV1::clone(&after_seal);
    let replayed = replay
        .apply_object(&reveal, &misaka_palw_kernel::route::AuthV1 { signer_bond: kid })
        .map(|_| ())
        .map_err(|e| format!("{e:?}"));
    let o = w.net.route(producer, &reveal);
    w.net.send(vec![(producer, o)]).await;
    if w.net.ledger().claims.contains_key(&produced.claim.id()) {
        return Ok(());
    }
    let state = w.net.chain.tip_state().1;
    let bindings: Vec<_> = state.kernel_route().map(|r| r.kernel_bindings_v1()).unwrap_or_default();
    let ours = bindings.iter().filter(|(_, b)| b.kernel_class == Hash64::from_bytes(w.class)).count();
    Err(format!(
        "bindings_total={} bindings_of_class={ours} salted={salted} salted_from={:?} ledger_daa={} sealed={sealed} ledger_replay(reveal)={replayed:?}",
        bindings.len(),
        after_seal.salted_seals_from(),
        after_seal.daa
    ))
}

#[tokio::test]
async fn vfuzz_claim_commit_bisect_the_17400_list() {
    kaspa_core::log::try_init_logger("error");
    let n = PALW_T12_G14_17000_FENCES_V1.len();
    let run = |k: usize, at: u64| async move {
        AssertUnwindSafe(probe_claim(prefix_config(k, at))).catch_unwind().await.map_err(panic_text)
    };
    eprintln!("VFUZZ-BISECT prefix=0/{n} -> {:?}", run(0, 0).await);
    eprintln!("VFUZZ-BISECT prefix={n}/{n} -> {:?}", run(n, 0).await);
    let (mut lo, mut hi) = (0usize, n);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        let r = run(mid, 0).await;
        let ok = matches!(r, Ok(Ok(())));
        eprintln!("VFUZZ-BISECT prefix={mid}/{n} ok={ok} {r:?}");
        if ok { lo = mid } else { hi = mid }
    }
    eprintln!("VFUZZ-BISECT first failing entry: #{hi} {}", PALW_T12_G14_17000_FENCES_V1[hi - 1].name);
    // only that entry (with nothing else from the list)
    let only = PALW_T12_G14_17000_FENCES_V1[hi - 1];
    let (config, _, premine, floats) = prefix_config(0, 0);
    let mut params = config.params.clone();
    palw_t12_arm_after_int13_at_v1(&mut params, &[only], Some(0));
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &params.palw_consensus_mode else { unreachable!() };
    let parts = (Config::new(params.clone()), b.clone(), premine, floats);
    let r = AssertUnwindSafe(probe_claim(parts)).catch_unwind().await.map_err(panic_text);
    eprintln!("VFUZZ-BISECT only {} -> {r:?}", only.name);
    // the full list armed later, crossed by the chain during setup (closer to a flag day than genesis arming)
    for at in [40u64, 400] {
        eprintln!("VFUZZ-BISECT full list at DAA {at} -> {:?}", run(n, at).await);
    }
}

/// **lane GC-DUTY option (C)**: every prefix of the 17,400 list (0..=n, armed at DAA 0) lets the honest Panel=1 claim commit.
#[tokio::test]
async fn gc_duty_every_prefix_of_the_17400_list_commits_the_honest_panel_one_claim() {
    kaspa_core::log::try_init_logger("error");
    let n = PALW_T12_G14_17000_FENCES_V1.len();
    let mut failed = Vec::new();
    for k in 0..=n {
        let r = AssertUnwindSafe(probe_claim(prefix_config(k, 0))).catch_unwind().await.map_err(panic_text);
        eprintln!("GC-DUTY prefix={k}/{n} -> {r:?}");
        if !matches!(r, Ok(Ok(()))) {
            failed.push((k, format!("{r:?}")));
        }
    }
    assert!(failed.is_empty(), "prefixes that do not commit: {failed:?}");
}
