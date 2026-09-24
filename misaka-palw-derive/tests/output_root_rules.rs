//! **testnet-12's output roots, recomputed here exactly as the chain recomputes them.**
//!
//! ADR-0152 v3.1 post-edit 4 unified the free-prompt lane's rendered rule with the attempt lane's:
//! past `palw_offence_attribution` every class's `output_root` is core's `CoreV1` commitment —
//! `palw_attempt_output_root_v1(ctx, ids)`, the empty rendering — and `OutputMismatch` (10) holds a
//! model class's free-prompt claim to it. This crate carried its own answer (the family's keyed
//! rendering), which is right on every other network and was wrong on testnet-12 for every model
//! class: an honest claim's derivation would have verified `output_root_matches: false`.
//!
//! The tests hold the crate to core's function — CALLED, never restated — over the classes
//! testnet-12 actually registers, read from its shipped preset rather than listed here, so a class
//! the card adds is covered the day it is added.

use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_attempt_rules_v1::{
    palw_attempt_canonical_v1, palw_attempt_context_v1, palw_attempt_output_root_v1, palw_attempt_prompt_root_v1,
    palw_attempt_rules_of_params_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;
use misaka_palw_base0::e2e_drill::PalwRcFamilyV1;
use misaka_palw_derive::{
    PalwAttemptRulesV1, attempt_rules_of_network_v1, recompute_output_root, recompute_output_root_of_context_v1,
    recompute_output_root_under_v1, rendered_output_hash_for_family, rendered_output_hash_under_v1,
};

fn net(suffix: u32) -> NetworkId {
    NetworkId::with_suffix(NetworkType::Testnet, suffix)
}

/// Every class testnet-12's genesis registers WITH an admission profile — the model classes (the
/// floor registers without one).
fn t12_model_classes(params: &Params) -> Vec<(Hash64, PalwShapeProfileV3)> {
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("testnet-12 ships a ConsensusV2 ruleset") };
    bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(carriage), .. } => {
                Some((*class_id, carriage.profile.clone()))
            }
            _ => None,
        })
        .collect()
}

/// `n` ids inside `vocab`, from a xorshift over `seed` — any ids a class could emit.
fn ids_in(vocab: u32, n: usize, seed: u64) -> Vec<u32> {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % u64::from(vocab.max(1))) as u32
        })
        .collect()
}

/// **The network names the rule, through the free-prompt worker's own derivation**: testnet-12
/// arms `CoreV1` and testnet-11 keeps `Legacy` — each exactly what core derives from the same
/// preset — and a string that is not a network, or a testnet suffix this build does not ship, is
/// refused rather than given a rule. The second reaches `Params::from`, which panics on it; the
/// refusal is that panic caught, so a verifier handed a stranger's string answers `Err`.
#[test]
fn the_network_names_the_rule_testnet_12_core_v1_and_testnet_11_legacy() {
    for (name, suffix, want) in [("testnet-12", 12, PalwAttemptRulesV1::CoreV1), ("testnet-11", 11, PalwAttemptRulesV1::Legacy)] {
        assert_eq!(attempt_rules_of_network_v1(name), Ok(want), "{name}");
        assert_eq!(palw_attempt_rules_of_params_v1(&Params::from(net(suffix))), want, "{name}: core derives the same rule");
    }
    assert!(attempt_rules_of_network_v1("palw-derive-not-a-network").is_err(), "a name that is not a network has no rule");
    let unshipped = attempt_rules_of_network_v1("testnet-99").expect_err("a suffix this build does not ship has no rule");
    assert!(unshipped.contains("testnet-99") && unshipped.contains("not a network this build ships"), "{unshipped}");
}

/// **On every model class testnet-12 registers, this crate's `output_root` IS the chain's**, for
/// every family a verifier might name (under `CoreV1` the family renders nothing into the root),
/// over the attempt context the chain derives for the class and over a free-prompt-shaped one,
/// through both the context form and the bare-hash form.
///
/// And the rule really moved: under `Legacy` a model family's root is a different value — the one
/// this crate used to compute, which the chain would convict as `OutputMismatch` — while the
/// floor's is the same under both rules. Without that half, a test that passed with the old rule in
/// place would be proving nothing.
#[test]
fn derive_recomputes_the_chains_root_on_every_testnet_12_model_class() {
    let params = Params::from(net(12));
    // The rule the verifier is told by name, through the one derivation — not a literal `CoreV1`:
    // the chain from `--network testnet-12` to the chain's root is what a verifier runs.
    let rules = attempt_rules_of_network_v1("testnet-12").expect("testnet-12 ships");
    assert_eq!(rules, PalwAttemptRulesV1::CoreV1);
    let classes = t12_model_classes(&params);
    // At least the two held rows the card registers today; a class it adds joins the loop below.
    assert!(classes.len() >= 2, "testnet-12 registers the held dense and the held hybrid row besides the floor: {}", classes.len());
    let form = params.palw_prompt_ids_form_v1();
    for (i, (class_id, profile)) in classes.iter().enumerate() {
        let canonical = palw_attempt_canonical_v1(profile, false).unwrap_or_else(|| panic!("{class_id}: no canonical job"));
        let anchor = Hash64::from_u64_word(0xD3E1_0012 + i as u64);
        let prompt_root = palw_attempt_prompt_root_v1(profile, &anchor, canonical.0, form).expect("a canonical prompt commits");
        let attempt = palw_attempt_context_v1(profile, &anchor, canonical, prompt_root);
        // A free-prompt context is another context over the same class: the worker's network name,
        // its own budget. The rule reads the context's hash and the ids, never which lane built it.
        let free_prompt = PalwJobContextV2 {
            network_id: b"testnet-12".to_vec(),
            declared_prefill_tokens: 26,
            exact_decode_tokens: 256,
            ..attempt.clone()
        };
        let contexts: [(&str, &PalwJobContextV2); 2] = [("attempt", &attempt), ("free-prompt", &free_prompt)];
        let id_sets = [
            ids_in(profile.vocab_size, canonical.1 as usize, 0xA7),
            ids_in(profile.vocab_size, 1, 0x01),
            ids_in(profile.vocab_size, 256, 0xF9),
        ];
        for (lane, ctx) in contexts {
            for ids in &id_sets {
                let chain = palw_attempt_output_root_v1(ctx, ids);
                for family in PalwRcFamilyV1::ALL {
                    let what = format!("{class_id} {lane} {} ids, {}", ids.len(), family.name());
                    assert_eq!(recompute_output_root_of_context_v1(rules, family, ctx, ids), chain, "{what}");
                    assert_eq!(
                        recompute_output_root_under_v1(rules, family, &ctx.context_hash(), ids),
                        chain,
                        "{what}: the bare-hash form is the same bytes"
                    );
                    // `Legacy` is the family's rendering, and the kept signature is `Legacy`.
                    let legacy = recompute_output_root_under_v1(PalwAttemptRulesV1::Legacy, family, &ctx.context_hash(), ids);
                    assert_eq!(legacy, recompute_output_root(family, &ctx.context_hash(), ids), "{what}");
                    assert_eq!(recompute_output_root_of_context_v1(PalwAttemptRulesV1::Legacy, family, ctx, ids), legacy, "{what}");
                    if family == PalwRcFamilyV1::Base0 {
                        assert_eq!(legacy, chain, "{what}: the floor renders nothing under either rule, so its roots do not move");
                    } else {
                        assert_ne!(legacy, chain, "{what}: a model family's Legacy root is not the one the chain holds");
                    }
                }
            }
        }
    }
}

/// **`CoreV1`'s rendering is core's, for every family; `Legacy`'s is the family's own.** The two
/// agree exactly on the floor — the fact the tool leans on when it recomputes a floor claim's root
/// without being told the network.
#[test]
fn core_v1_renders_nothing_and_legacy_renders_the_family() {
    for ids in [&[][..], &[1u32][..], &[3, 5, 8, 13, 21][..], &[151_643, 872, 15_339][..]] {
        let core = kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_rendered_output_v1(ids);
        for family in PalwRcFamilyV1::ALL {
            assert_eq!(rendered_output_hash_under_v1(PalwAttemptRulesV1::CoreV1, family, ids), core, "{}", family.name());
            assert_eq!(
                rendered_output_hash_under_v1(PalwAttemptRulesV1::Legacy, family, ids),
                rendered_output_hash_for_family(family, ids),
                "{}",
                family.name()
            );
        }
        assert_eq!(rendered_output_hash_for_family(PalwRcFamilyV1::Base0, ids), core, "the floor's two rules are one");
    }
}

/// **The output commitment, pinned to the literals the stranger holds** (`scripts/
/// misaka-palw-derive-stranger.py`: `CORE_V1_RENDERED_OUTPUT_HEX`, `CORE_V1_OUTPUT_ROOT_HEX` and
/// `LEGACY_QWEN36_OUTPUT_ROOT_HEX`, checked by its `selftest`). The stranger's `output_commitment_v2`
/// framed its buffer with an outer u64 length until 2026-09-24 and so never agreed with the chain;
/// its selftest only round-tripped its own value. Now the independent verifier and core cannot
/// drift apart without one of the two going red — for the CoreV1 rule and for a Legacy family.
#[test]
fn the_output_commitment_is_the_strangers_pinned_literal() {
    const CORE_V1_RENDERED_OUTPUT_HEX: &str = "7fdc18652f51bc53dc873f4ae569c4f9f1e27b222a93d0753ab1948d8a35a8415c129aebeff6217b39d9fecc4fd5fa5ac8eec5ccfd2bf79fca9008165c4cbee9";
    const CORE_V1_OUTPUT_ROOT_HEX: &str = "4b535b595041f77f126c83c03fc705428c079f3ba147a5b50cc54212c01299395efbdb61e8ce2b4a858d47084f6903f89cb3c86400da67b959d6a0a388295dcb";
    const LEGACY_QWEN36_OUTPUT_ROOT_HEX: &str = "2d24e97b82fdb2594a6a1b9fd6871d43a5d89e374854cd3d758596f2ba95f8d0c78f85a27c4cac91bafa7ab2fea8b3f7101725aa34e1a21ba9dbf21eebff8cef";
    let ids = [1u32, 2, 3];
    let ctx = Hash64::from_bytes([0x11; 64]);
    let rendered = kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_rendered_output_v1(&ids);
    assert_eq!(rendered.to_string(), CORE_V1_RENDERED_OUTPUT_HEX, "CoreV1's rendered hash");
    let root = kaspa_consensus_core::palw_v2::output_commitment_v2(&ctx, &ids, &rendered);
    assert_eq!(root.to_string(), CORE_V1_OUTPUT_ROOT_HEX, "a CoreV1 root");
    let legacy = kaspa_consensus_core::palw_v2::output_commitment_v2(
        &ctx,
        &ids,
        &misaka_palw_base0::qwen36_backend::rendered_output_hash_v1(&ids),
    );
    assert_eq!(legacy.to_string(), LEGACY_QWEN36_OUTPUT_ROOT_HEX, "a Legacy qwen36 root");
}
