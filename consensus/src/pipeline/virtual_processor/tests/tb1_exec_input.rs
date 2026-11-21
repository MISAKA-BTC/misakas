//! Held-model exercise for the private EXEC input reader. Registration consumes
//! the artifact/class identities. The old Studio claim never supplies a job,
//! authorization, reserved funds, ticket or result to this run.
use super::*;
use kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_root_v1;
use misaka_palw_remote::tb1_exposure::exec_input::{DerivedExecInput, ExecInputTerms};
use misaka_palw_tir_exec::node::TirArtifactV1;

const MODEL_ROOT: &str =
    "8e31580dc81f6548b9e440c9b6d45fe63bff1d42ed2dbb4b342bdb58fc4590f0b8210f10a2a23e6c352c866a4313004b547ec363997e5757d2bce31a6838eec4";
const CLASS_ID: &str =
    "7a2616b4b132946d21b1d396c0a82f69a144fb7f692394314efb35e3f8bff1f231b857f174e1165ad92c65ce742e1bcc57dfb11e1e5eab02b67b8cf87436c625";
const TOKENIZER: &str =
    "57c55fb228e8bb272c1d3e3a75eb8e7a238fe2e6a7ccc5e4cd6b874c1278fbc79342c2d466bc52d53000bb0de6de57cb196fc0a413c2751d45cfb69551615ac2";

pub(super) fn terms(anchor: Hash64) -> ExecInputTerms<'static> {
    let mut target = [0; 33];
    target[1] = 0x80; // Private strict 256-bit ticket threshold. This test issues no ticket.
    ExecInputTerms {
        qf1: None,
        registration_anchor: anchor,
        candidate_set: [18; 64],
        beneficiary: [19; 64],
        model_root: MODEL_ROOT.parse().unwrap(),
        class_id: CLASS_ID.parse().unwrap(),
        tokenizer: TOKENIZER.parse().unwrap(),
        job_tokens: &[42, 108, 17],
        vocab: 49152,
        max_positions: 8192,
        decode_tokens: 1,
        max_leaves: 400_000,
        nonce: [20; 32],
        policy: [17; 64],
        difficulty: [21; 64],
        target,
        accounting: [22; 64],
        moe2_quote: None,
        open_daa: 0,
        deadline_exclusive_daa: 40,
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(super) struct Inference {
    generated: Vec<u32>,
    input: Hash64,
    execution: Hash64,
    trace: Hash64,
    output: Hash64,
    steps: u64,
}

pub(super) fn infer(native: &dyn kaspa_consensus_core::api::ConsensusApi, path: &Path, input: &DerivedExecInput<'_>) -> Inference {
    let artifact = TirArtifactV1::open(path).unwrap();
    let run = input.replay_tir(native, &artifact).unwrap();
    assert_eq!(run.generated.len(), input.terms().decode_tokens as usize);
    assert_eq!(run.leaf_count, run.leaf_hashes.len() as u64);
    Inference {
        generated: run.generated,
        input: prompt_token_ids_root_v1(input.tokens()).unwrap(),
        execution: run.execution_root,
        trace: run.trace_root,
        output: run.output_root,
        steps: run.leaf_count,
    }
}

pub(super) fn check_quote_substitutions(
    native: &dyn kaspa_consensus_core::api::ConsensusApi,
    preparation: Hash64,
    rounds: &QualifiedExecPanel,
    entry: &SignedExecInput<'_>,
    quote: kaspa_consensus_core::palw_model_market_v1::moe2::settlement::Moe2PreDrawQuote,
) {
    use kaspa_consensus_core::palw_model_market_v1::moe2::{
        Moe2PriceLimits, Moe2QuantityLimits, Moe2WorkerPlan, settlement::Moe2PreDrawQuote,
    };
    let refused = |row| ExecInputBatch::from_signed(native, preparation, rounds, &[row], 4, 16).is_err();
    assert!(refused(SignedExecInput { quote_consent: None, ..*entry }));
    assert!(refused(SignedExecInput { slot: entry.slot + 1, ..*entry }));
    assert!(refused(SignedExecInput { funding_ref: [0; 64], ..*entry }));
    for changed in [
        Moe2PreDrawQuote { deadline: quote.deadline + 1, ..quote },
        Moe2PreDrawQuote { quote_anchor: preparation, ..quote },
        Moe2PreDrawQuote { plan: Moe2WorkerPlan::Miner, ..quote },
        Moe2PreDrawQuote { policy: Hash64::default(), ..quote },
        Moe2PreDrawQuote { model: Hash64::default(), ..quote },
        Moe2PreDrawQuote { limits: Moe2QuantityLimits { min_out: 1, ..quote.limits }, ..quote },
        Moe2PreDrawQuote { price_limits: Moe2PriceLimits { max_output_slippage_bps: 1, ..quote.price_limits }, ..quote },
    ] {
        let changed_terms = ExecInputTerms { moe2_quote: Some(changed), ..*entry.terms };
        assert!(
            ExecInputBatch::from_signed(native, preparation, rounds, &[SignedExecInput { terms: &changed_terms, ..*entry }], 4, 16)
                .is_err()
        );
    }
    use misaka_palw_remote::tb1_exposure::exec_input::moe2::{QUOTE_DOMAIN, consent_ref};
    let key = entry.terms.registration_key(preparation, rounds, entry.owner, 16).unwrap();
    let moved = SignedExecInput { slot: entry.slot + 1, funding_ref: consent_ref(key, entry.slot + 1), ..*entry };
    assert_eq!(
        ExecInputBatch::from_signed(native, preparation, rounds, &[moved], 4, 16).err().unwrap().to_string(),
        "invalid MOE2 funder consent"
    );
    let other_bond = crate::consensus::test_consensus::TestConsensus::palw_v2_registry_keypair(2);
    let other_sig = libcrux_ml_dsa::ml_dsa_87::sign(&other_bond.signing_key, &entry.funding_ref, QUOTE_DOMAIN, [47; 32]).unwrap();
    let substituted = SignedExecInput { quote_consent: Some(&other_sig), ..*entry };
    assert_eq!(
        ExecInputBatch::from_signed(native, preparation, rounds, &[substituted], 4, 16).err().unwrap().to_string(),
        "invalid MOE2 funder consent"
    );
    let mut sig = entry.quote_consent.unwrap().as_slice().to_vec();
    sig[0] ^= 1;
    let wrong = libcrux_ml_dsa::ml_dsa_87::MLDSA87Signature::new(sig.as_slice().try_into().unwrap());
    assert!(
        ExecInputBatch::from_signed(native, preparation, rounds, &[SignedExecInput { quote_consent: Some(&wrong), ..*entry }], 4, 16)
            .is_err()
    );
}
