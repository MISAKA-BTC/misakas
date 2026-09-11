//! **ADR-0110 Decision 5's external vectors, pinned** — the widths CI does not run.
//!
//! Its own test target rather than a row in `context_vector`'s module, on purpose: the
//! release-mode vector job runs `cargo test --release -p misaka-palw-base0 --lib -- --ignored
//! context_vector`, and its gate pins the count of tests that match at two. A pin that matched
//! that filter would either lengthen CI by the better part of an hour or turn the gate red. Here
//! the filter cannot see it (`--lib` builds no integration test), and the pin is run by hand:
//!
//! ```text
//! cargo test --release -p misaka-palw-base0 --test context_vector_external -- --ignored
//! ```
//!
//! A pin is a reproduction: the id below is what one host printed the first time the vector ran
//! to completion, and the ADR's §9.5 says which host and how long. The document is pinned because
//! a reproduction exists, not because enough of them agreed (ADR-0110 Decision 5).

use misaka_palw_base0::context_vector::{
    PalwContextRulesetV1, PalwContextStageV1, PalwContextVerdictV1, palw_context_vector_v1, palw_verify_context_vector_v1,
};

fn check_pinned(name: &str, pinned_document_id: &str) {
    let ruleset = PalwContextRulesetV1::devnet_held_v1().expect("the held devnet");
    let vector = palw_context_vector_v1(name).expect("shipped");
    let f = palw_verify_context_vector_v1(&vector, &ruleset, &PalwContextStageV1::ALL);
    for (stage, verdict) in &f.verdicts {
        assert_eq!(verdict, &PalwContextVerdictV1::Pass, "{name}: stage {} did not pass", stage.name());
    }
    assert_eq!(f.verdicts.len(), PalwContextStageV1::ALL.len(), "{name}: every stage ran");
    let tamper = f.tamper.as_ref().expect("the court's tampered half ran");
    assert_eq!((tamper.named, tamper.verdict.as_str()), (Some(0), "ExecutorGuilty"), "{name}: invariant 4");
    assert!(f.court.iter().all(|l| l.verdict == "FalseAccusation"), "{name}: honest leaves clear");
    assert_eq!(f.document_id().to_string(), pinned_document_id, "{name}: the document moved — re-pin with the reason");
}

/// **The 131,072-position vector.** First reproduced 2026-09-11 on a 12-core M-series host: 44.7
/// minutes, 7.5 GB peak resident (ADR-0110 §9.5). Every stage passed; the court's tampered half
/// ran (13.1 M leaves, inside the 2^24 dense re-execution cap).
#[test]
#[ignore = "an external run: about 45 minutes in release on a 12-core host"]
fn the_128k_vector_passes_every_stage_and_is_pinned() {
    check_pinned(
        "0110-dense-v7-128k",
        "7bc3f88b60bd2857cf733ac316d16109723d161a5cd4a5bb924edd7bbbdf688a9a42e99bd1fc91d1e8e5f6f95ab2009827e685a025caafae50b336a14e7cabe0",
    );
}
