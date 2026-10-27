//! **lane J1 (2026-10-27): the user journey on the Panel=1 kernel route, text in → text out.** The CLI's own builder
//! (`misaka_palw_sdk::kernel_job_v1`, what `misaka palw job post` calls) tokenises a prompt with the user's tokenizer into a
//! `PostJob`; the producer claims it, the Panel=1 seat covers it, it Finals, Gate C's drawn verifiers certify it; at every stage the
//! op 227 views (`kernel-classes`, `kernel-job`, `kernel-result`) read it, and the result renders back to text with the same
//! tokenizer — provisional before SecurityCertified, certified after. TEST-ONLY Gate C policy (see `cert.rs`).

use super::cert::*;
use super::*;
use kaspa_consensus_core::palw_user_job_views_v1::*;
use kaspa_consensus_core::palw_user_views_v1::PalwUserViewClockV1;
use misaka_palw_sdk::kernel_job_v1::{kernel_job_from_text_v1, kernel_result_text_v1, kernel_tokenizer_v1, tests::tiny_tokenizer_json};

fn clock(net: &Net) -> PalwUserViewClockV1 {
    PalwUserViewClockV1 { next_daa: net.daa() + 1, raw_depth: None, objective_offence: false }
}

fn job_view(net: &Net, job: &Hash64) -> serde_json::Value {
    palw_kernel_job_json_v1(&net.chain.tip_state().1, job, &clock(net)).expect("the job is held")
}

fn result_view(net: &Net, job: &Hash64) -> serde_json::Value {
    palw_kernel_result_json_v1(&net.chain.tip_state().1, job, &clock(net)).expect("the job is held")
}

#[tokio::test]
async fn j1_user_journey_text_prompt_to_certified_text_result_on_the_panel_one_kernel_route() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::on(Net::over_cfg(cert_panel_one_config(), TestConsensus::new)).await;
    let tok = kernel_tokenizer_v1(&tiny_tokenizer_json(), None).unwrap();

    // ---- view (a): the class list names the fixture class with its bounds ----
    let classes = palw_kernel_classes_json_v1(&w.net.chain.tip_state().1, &w.net.bundle.state, &clock(&w.net)).expect("kernel route");
    let class_id = Hash64::from_bytes(w.class).to_string();
    let row = classes["classes"].as_array().unwrap().iter().find(|c| c["classId"] == class_id).expect("the class is listed").clone();
    assert_eq!(row["tokenizerCommitment"], serde_json::Value::Null, "a kernel class commits to no tokenizer");
    let (bound, positions) = (row["tokenBound"].as_u64().unwrap() as u32, row["maxPositions"].as_u64().unwrap() as u32);

    // ---- the CLI builder: text → PostJob ----
    let prompt = "hi a";
    let job = kernel_job_from_text_v1(&tok.tokenizer, prompt, Hash64::from_bytes(w.class), bound, positions, 3, Hash64::from_bytes([0x71u8; 64]))
        .expect("the class admits the job");
    assert_eq!(kernel_result_text_v1(&tok.tokenizer, &job.prompt).unwrap(), prompt, "tokenisation round-trips");
    let o = w.net.route(1, &K::PostJob { job: job.clone() });
    w.net.send(vec![(1, o)]).await;
    let job_id = Hash64::from_bytes(job.id());
    let v = job_view(&w.net, &job_id);
    assert_eq!((v["status"].as_str(), v["escrow"]["state"].as_str()), (Some("pending"), Some("reserved")), "{v}");
    assert_eq!(result_view(&w.net, &job_id)["standing"], "none");

    // ---- producer claims, the Panel=1 seat covers ----
    let claim = w.claim(0, &job, false).await;
    w.until_seated_v1(&claim.id).await;
    w.cover(&claim.id).await;
    assert_eq!(job_view(&w.net, &job_id)["status"], "claimed");

    // ---- Final: the result is readable, provisional ----
    let floor = final_floor(&w, Mode::Panel, &claim.id);
    w.net.beat_to(floor).await;
    let v = job_view(&w.net, &job_id);
    assert_eq!(v["status"], "final", "{v}");
    let r = result_view(&w.net, &job_id);
    assert_eq!(r["standing"], "provisional", "{r}");
    let tokens: Vec<u32> = r["tokens"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as u32).collect();
    assert_eq!(tokens.len(), 3, "exactly max_new generated tokens");
    let text = kernel_result_text_v1(&tok.tokenizer, &tokens).expect("the fixture's ids render under the user's tokenizer");
    assert_eq!(tok.tokenizer.encode_without_specials(&text).unwrap(), tokens, "the result text round-trips to the claim's ids");

    // ---- Gate C: the drawn verifiers certify; the result turns certified, the job settles ----
    let row = until_drawn(&mut w.net, &claim.id).await;
    for (card, _) in drawn_cards(&w.net, &row) {
        attest(&mut w.net, &claim.id, card, misaka_palw_kernel::model_eligibility_v1::EligibilityVerdictV1::Reproduced).await;
    }
    until_decided(&mut w.net, &claim.id, TEST_ONLY_KERNEL_CERT_POLICY_V1.prosecution_window_daa + 50).await;
    let r = result_view(&w.net, &job_id);
    assert_eq!((r["standing"].as_str(), r["securityCertified"].as_bool()), (Some("certified"), Some(true)), "{r}");
    let v = job_view(&w.net, &job_id);
    assert_eq!((v["status"].as_str(), v["escrow"]["state"].as_str()), (Some("settled"), Some("paid")), "{v}");
}
