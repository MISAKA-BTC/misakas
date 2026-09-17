//! **A class's context length, and where the answer came from.**
//!
//! A client sizing a prompt for a class (MISAKA Studio, a gateway, an operator) needs the class's
//! `n_ctx` — the cached positions every inference of the class runs in — and nothing published it: `--palw-dump-classes` and `getPalwClasses` print share, budget and leaves,
//! and the class state holds no profile. The number exists in two places a node can read, and both
//! are the class's own graph, because a class id IS its shape profile's hash:
//!
//! * **the chain's registration** — the carriage an accepted registration carried, indexed by
//!   ADR-0067 and re-checked against the id and the registered artifact root on every read
//!   (`palw_registered_class_carriage_v1`);
//! * **this build's class ledger** — every class the build's lineages can supply
//!   (`PalwClassSdk::ledger`), which is where a genesis class registered without a carriage has
//!   its graph at all.
//!
//! The chain's answer wins where it exists; the ledger fills the rest; a class neither knows is
//! reported as unknown rather than given a default, since a guessed window is the defect that cut
//! Studio's answers at 512 tokens while it believed 4,096.
//!
//! **Two budget rules read this one window, and they differ by one position.** A job of `prefill`
//! prompt tokens and `decode` generated tokens caches `prefill + decode − 1` positions: the first
//! generated token is sampled from the prefill's last logits, so the job makes `decode − 1` decode
//! calls (`palw_step::kv_aux_leaf_count`). That FOOTPRINT is what the step trace, the court and class
//! admission bound by `n_ctx`, and a footprint of exactly `n_ctx` is the declared worst case
//! ([`palw_job_footprint_v1`]). The canonical attempt job is sized that way, so the Qwen3.6 hybrid's
//! `(7, 2)` sits exactly at its `n_ctx` of 8 — nine tokens, eight positions, by design. A free-prompt
//! job is admitted by the stricter `prompt_tokens + decode_token_limit ≤ max_context_tokens`, with
//! `max_context_tokens ≤ n_ctx` (`palw_freeprompt_v3`, `fp_worker`), so a client sizing its own
//! prompt for that lane gets at most `n_ctx − prompt` tokens of answer.
//! [`PalwClassContextV1::canonical_footprint_positions`] reports the first rule; the second is the
//! client's arithmetic on `n_ctx`.

use std::collections::HashMap;

use kaspa_consensus_core::{
    config::params::Params, palw_context_ladder::palw_job_footprint_v1, palw_mode_v2::PalwConsensusMode,
    palw_step::PalwShapeProfileV3, palw_v2::PalwJobContextV2,
};
use kaspa_hashes::Hash64;

/// Where a [`PalwClassContextV1`] was read from.
pub const PALW_CLASS_CONTEXT_SOURCE_CHAIN: &str = "chain_registration";
pub const PALW_CLASS_CONTEXT_SOURCE_LEDGER: &str = "build_ledger";
pub const PALW_CLASS_CONTEXT_SOURCE_UNKNOWN: &str = "unknown";

/// One class's context, with its provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwClassContextV1 {
    pub class_id: Hash64,
    /// The human identity this build's ledger gives the class; empty where the build does not
    /// supply it.
    pub model_id: String,
    /// The class's window in cached positions, the bound of every job's footprint. Zero when unknown.
    pub n_ctx: u32,
    /// The canonical job the class is paid per.
    pub canonical_prefill_tokens: u32,
    pub canonical_decode_tokens: u32,
    /// The positions that job caches, `prefill + max(1, decode) − 1` ([`palw_job_footprint_v1`]):
    /// never above `n_ctx`, and equal to it for a class whose job is sized to the window.
    pub canonical_footprint_positions: u32,
    /// The canonical job's declared context bound.
    pub max_context_tokens: u32,
    /// [`PALW_CLASS_CONTEXT_SOURCE_CHAIN`], [`PALW_CLASS_CONTEXT_SOURCE_LEDGER`] or
    /// [`PALW_CLASS_CONTEXT_SOURCE_UNKNOWN`].
    pub source: &'static str,
}

/// Consensus's footprint, narrowed for the report: it is at most `u32::MAX` for `u32` inputs.
fn footprint_positions(prefill: u32, decode: u32) -> u32 {
    u32::try_from(palw_job_footprint_v1(prefill, decode)).unwrap_or(u32::MAX)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LedgerRow {
    model_id: String,
    n_ctx: u32,
    canonical_prefill_tokens: u32,
    canonical_decode_tokens: u32,
    max_context_tokens: u32,
}

/// **This build's class ledger, indexed by class id** — built once from the network's own court and
/// prompt-commitment form, exactly as the producer builds its SDK, so a class id here is the id the
/// chain registers for that graph on this network.
#[derive(Clone, Debug, Default)]
pub struct PalwBuildClassLedgerV1 {
    rows: HashMap<Hash64, LedgerRow>,
}

impl PalwBuildClassLedgerV1 {
    /// `None` on a network without a V2 bundle, which has no PALW classes to describe.
    pub fn from_params(params: &Params) -> Option<Self> {
        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { return None };
        let sdk = misaka_palw_sdk::PalwClassSdk::builtin_v1(
            bundle.court,
            params.palw_prompt_ids_form_v1(),
            params.net.to_string().into_bytes(),
        );
        let rows = sdk
            .ledger()
            .into_iter()
            .map(|entry| {
                let canonical = entry.canonical_context();
                (
                    entry.class_id(),
                    LedgerRow {
                        model_id: entry.model_id.to_string(),
                        n_ctx: entry.profile.n_ctx,
                        canonical_prefill_tokens: canonical.declared_prefill_tokens,
                        canonical_decode_tokens: canonical.exact_decode_tokens,
                        max_context_tokens: canonical.max_context_tokens,
                    },
                )
            })
            .collect();
        Some(Self { rows })
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The context of `class_id`: the chain's registration when the node holds one (`chain`, from
    /// `palw_registered_class_carriage_v1`), else this ledger's entry, else unknown.
    pub fn class_context(&self, class_id: Hash64, chain: Option<(PalwShapeProfileV3, PalwJobContextV2)>) -> PalwClassContextV1 {
        let ledger = self.rows.get(&class_id);
        let model_id = ledger.map(|row| row.model_id.clone()).unwrap_or_default();
        match (chain, ledger) {
            (Some((profile, canonical)), _) => PalwClassContextV1 {
                class_id,
                model_id,
                n_ctx: profile.n_ctx,
                canonical_prefill_tokens: canonical.declared_prefill_tokens,
                canonical_decode_tokens: canonical.exact_decode_tokens,
                canonical_footprint_positions: footprint_positions(canonical.declared_prefill_tokens, canonical.exact_decode_tokens),
                max_context_tokens: canonical.max_context_tokens,
                source: PALW_CLASS_CONTEXT_SOURCE_CHAIN,
            },
            (None, Some(row)) => PalwClassContextV1 {
                class_id,
                model_id,
                n_ctx: row.n_ctx,
                canonical_prefill_tokens: row.canonical_prefill_tokens,
                canonical_decode_tokens: row.canonical_decode_tokens,
                canonical_footprint_positions: footprint_positions(row.canonical_prefill_tokens, row.canonical_decode_tokens),
                max_context_tokens: row.max_context_tokens,
                source: PALW_CLASS_CONTEXT_SOURCE_LEDGER,
            },
            (None, None) => PalwClassContextV1 {
                class_id,
                model_id,
                n_ctx: 0,
                canonical_prefill_tokens: 0,
                canonical_decode_tokens: 0,
                canonical_footprint_positions: 0,
                max_context_tokens: 0,
                source: PALW_CLASS_CONTEXT_SOURCE_UNKNOWN,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::network::{NetworkId, NetworkType};

    /// **The ledger names every class the build supplies by the id the chain registers**, with a
    /// window that bounds its canonical job, and the chain's registration outranks it.
    #[test]
    fn the_ledger_answers_by_class_id_and_the_chain_outranks_it() {
        let params = Params::from(NetworkId::new(NetworkType::Devnet));
        let ledger = PalwBuildClassLedgerV1::from_params(&params).expect("devnet carries a V2 bundle");
        assert!(!ledger.is_empty(), "the build supplies at least the floor");
        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { unreachable!() };
        let floor = ledger.class_context(bundle.base_class_id, None);
        assert_eq!(floor.source, PALW_CLASS_CONTEXT_SOURCE_LEDGER, "the devnet floor is registered without a carriage");
        assert!(floor.n_ctx > 0, "a known class has a window");
        assert!(
            floor.canonical_footprint_positions > 0 && floor.canonical_footprint_positions <= floor.n_ctx,
            "the canonical job's footprint fits the window: {floor:?}"
        );
        // Every row's canonical job fits its window by the rule the step trace and admission bound
        // it by — the FOOTPRINT, `prefill + decode − 1` — not by the free-prompt lane's stricter
        // `prompt + decode ≤ max_context`. The Qwen3.6-35B-A3B row is the case that tells them
        // apart: (7, 2) at n_ctx 8 is a footprint of exactly 8, the declared worst case, and the
        // free-prompt reading turns it into a nine-token job overflowing an eight-position window,
        // which is the misreading this test first shipped with.
        for (class_id, row) in &ledger.rows {
            let context = ledger.class_context(*class_id, None);
            assert!(
                context.n_ctx > 0 && context.canonical_footprint_positions <= context.n_ctx,
                "{class_id}: the canonical footprint exceeds the window: {context:?}"
            );
            assert_eq!(context.model_id, row.model_id);
        }
        let hybrid = ledger
            .rows
            .iter()
            .find(|(_, row)| row.model_id == "Qwen3.6-35B-A3B")
            .map(|(class_id, _)| ledger.class_context(*class_id, None))
            .expect("this build supplies the hybrid");
        assert_eq!(
            (hybrid.canonical_prefill_tokens, hybrid.canonical_decode_tokens, hybrid.n_ctx, hybrid.canonical_footprint_positions),
            (7, 2, 8, 8),
            "the hybrid's canonical job sits exactly at its window; if these numbers move, re-read the rule above"
        );

        // The chain's registration outranks the ledger: the same class answered from a carriage
        // reports the carriage's numbers and says so.
        let sdk = misaka_palw_sdk::PalwClassSdk::builtin_v1(
            bundle.court,
            params.palw_prompt_ids_form_v1(),
            params.net.to_string().into_bytes(),
        );
        let entry = sdk.ledger().into_iter().next().expect("the build supplies a class");
        let mut canonical = entry.canonical_context();
        canonical.max_context_tokens = entry.profile.n_ctx;
        let from_chain = ledger.class_context(entry.class_id(), Some((entry.profile.clone(), canonical.clone())));
        assert_eq!(from_chain.source, PALW_CLASS_CONTEXT_SOURCE_CHAIN);
        assert_eq!(
            (from_chain.n_ctx, from_chain.canonical_prefill_tokens, from_chain.canonical_decode_tokens),
            (entry.profile.n_ctx, canonical.declared_prefill_tokens, canonical.exact_decode_tokens)
        );
        assert_eq!(from_chain.model_id, entry.model_id, "the ledger still names a class the chain answered for");

        let unknown = ledger.class_context(Hash64::from_u64_word(0xDEAD), None);
        assert_eq!(
            (unknown.source, unknown.n_ctx),
            (PALW_CLASS_CONTEXT_SOURCE_UNKNOWN, 0),
            "no default window for a class nobody knows"
        );
    }
}
