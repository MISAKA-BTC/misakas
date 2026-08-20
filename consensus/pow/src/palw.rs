//! PALW (`algo_id = 4` / `5`) Layer-1 tag path — the PURE half.
//!
//! One PoW attempt = one full inference under the V1 lineage: the 32-byte seed
//! (`pow_layer0::palw_pow_seed_v1` over network ∥ pre-PoW hash ∥ timestamp ∥ nonce) renders the
//! canonical prompt and the replay-stable projection fields become the Layer-1 tag. Verification
//! IS re-execution — which is exactly what ADR-0042 Decision 4 forbids the consensus build from
//! CONTAINING. So this module keeps only what is checkable without a model:
//!
//! * seed derivation and the devnet-only **fixture tag family** (a deterministic, model-free
//!   rule set, confined per network where the tag is computed);
//! * the calibration **policy** — which networks pin a determinism class, and the refusal of the
//!   fixture family on a class-pinned network;
//! * a set-once [`PalwPowRuntime`] slot that a composition root (kaspad's startup rail on a
//!   network whose rules can demand algo 4/5, a miner grinding such templates) fills with the
//!   impure driver (`misaka-palw-pow-driver`).
//!
//! Everything that REACHES a model — `palw-worker` subprocesses, the resident pow-agent pool,
//! the host-local Ollama HTTP runner, tag caches, the host-wide inference lease — lives in that
//! driver crate. With nothing registered, an inference-priced tag is
//! [`PowLayer0Error::PalwUnavailable`] and the header fails PoW: **a full node without a model
//! is the normal case, not a fault** (ADR-0042 Decision 4). A CI test on the dependency graph
//! (`no_model_runtime_edge.rs`) keeps the consensus crates free of any edge back to the driver
//! or to the runtime crates behind it.
//!
//! Modes, in resolution order:
//!  1. `MISAKA_PALW_POW_FIXTURE=1` — the in-process fixture tag
//!     (`pow_layer0::palw_fixture_l1_tag_v1`): CI/harness runs without the 1.2 GB model. A
//!     fixture node and a real-model node are DIFFERENT rule sets (different tags) and must not
//!     share a mesh — the `devnet-vlt-fixture` precedent.
//!  2. A registered [`PalwPowRuntime`] — the real pinned runtime, behind the driver.
//!  3. Neither — [`PowLayer0Error::PalwUnavailable`], and the header fails PoW. Networks whose
//!     required-algo rule demands an inference-priced id refuse to boot a kaspad without a
//!     verified runtime (the startup rail), so the nodes that reach this case are ones the
//!     header was never valid for.

use kaspa_consensus_core::pow_layer0::{
    POW_L1_PALW_OLLAMA_OUT_BYTES, POW_L1_PALW_OUT_BYTES, PowLayer0Error, palw_fixture_l1_tag_v1, palw_ollama_fixture_l1_tag_v1,
    palw_pow_seed_v1, palw_worker_calibration_v1,
};
use kaspa_hashes::Hash64;
use std::sync::OnceLock;

/// `"1"` selects the in-process fixture tag (no model, no subprocess).
pub const PALW_FIXTURE_ENV: &str = "MISAKA_PALW_POW_FIXTURE";
/// Upper bound on PALW inferences in flight in this process (default [`DEFAULT_CONCURRENCY`]).
///
/// Header validation is a burst load exactly once — a from-genesis sync, where a pruning proof buys
/// one inference per header — and a trickle forever after (one per block interval). This knob is
/// for the burst (ADR-0041 Decision 2). It changes NOTHING about what is accepted, only how many
/// workers may run at once, and it costs memory linearly: each concurrent inference holds a
/// resident 1.2 GB model, so `N` here means roughly `N × 1.4 GiB`.
pub const PALW_CONCURRENCY_ENV: &str = "MISAKA_PALW_CONCURRENCY";
/// One — the serialized behaviour this path had before Decision 2. It stays the default because
/// raising it is a memory decision only an operator can make.
pub const DEFAULT_CONCURRENCY: usize = 1;

/// How many PALW inferences this process may run at once. Read once, on first use.
///
/// Lives HERE rather than in the driver because it is also the batch size the pruning-proof
/// validator prefetches header PoW in, so the bound and its consumer cannot drift apart; the
/// driver reads the same resolved value for its own gate and for the host-wide lease.
pub fn inference_concurrency() -> usize {
    static RESOLVED: OnceLock<usize> = OnceLock::new();
    *RESOLVED.get_or_init(|| {
        std::env::var(PALW_CONCURRENCY_ENV)
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|n| *n >= 1)
            .unwrap_or(DEFAULT_CONCURRENCY)
    })
}

/// Whether the operator asked for the fixture family AT ALL — a process-global answer, because
/// the variable is process-global.
///
/// Almost every caller wants [`fixture_enabled_for`] instead. This one exists for the two places
/// that must react to the REQUEST rather than to its effect: the kaspad startup rail, which tells
/// an operator their variable will not do what they think, and [`verify_worker_calibration`].
pub fn fixture_requested() -> bool {
    std::env::var(PALW_FIXTURE_ENV).as_deref() == Ok("1")
}

/// Whether the fixture tag family is permitted to be the rule set of `network_id`.
///
/// **Devnet only**, which is the same rule the kaspad rail has always stated — moved to where the
/// tag is COMPUTED, because that is where it can be enforced per network instead of per process.
/// The fixture derives different tags than the pinned model, so a fixture node on a real PALW
/// network forks at its first block and its blocks are invalid to every peer.
///
/// Two things follow from making this a function of the network rather than of the process:
///
/// * **Nothing can skip it.** The old confinement lived in kaspad's startup rail, so a miner, a
///   harness or any other library consumer of [`palw_l1_tag`] was never subject to it. This is the
///   same argument [`verify_worker_calibration`] already makes for checking lazily on the tag path.
/// * **One process may host several networks.** A test binary that runs a devnet consensus and a
///   simnet consensus is not a misconfiguration, and it used to be impossible: the variable the
///   devnet side requires aborted the process on behalf of the simnet side, which never computes a
///   PALW tag at all (`SIMNET_PARAMS.pow_palw_activation` is `never()`).
///
/// Matches the `NetworkId` display form the consensus layer passes down
/// (`params.net.to_string()`), so a suffixed devnet (`devnet-3`) is devnet and a network merely
/// containing the word (`kaspa-devnet`) is not.
pub fn fixture_permitted_on(network_id: &[u8]) -> bool {
    network_id == b"devnet" || network_id.starts_with(b"devnet-")
}

/// Whether the fixture tag is what THIS network's rules are, in this process.
pub fn fixture_enabled_for(network_id: &[u8]) -> bool {
    fixture_requested() && fixture_permitted_on(network_id)
}

/// The impure half of the legacy (algo 4/5) tag path — everything that reaches a model runtime.
///
/// The consensus build defines the SLOT and nothing that could fill it (ADR-0042 Decision 4):
/// the one implementation lives in `misaka-palw-pow-driver`, and only composition roots that
/// want the legacy lane link it. Implementations own their caching, retry policy, concurrency
/// gating and probe memoization; this trait's contract is only that a tag is a pure function of
/// the seed — every conforming runtime for a network computes the same bytes or errors.
pub trait PalwPowRuntime: Sync {
    /// The 200-byte algo-4 tag for one seed (one pinned-worker inference, or a cache hit).
    fn tag_for_seed(&self, seed: &[u8; 32]) -> Result<[u8; POW_L1_PALW_OUT_BYTES], PowLayer0Error>;
    /// The 72-byte algo-5 tag for one seed (one Ollama inference, or a cache hit).
    fn ollama_tag_for_seed(&self, seed: &[u8; 32]) -> Result<[u8; POW_L1_PALW_OLLAMA_OUT_BYTES], PowLayer0Error>;
    /// Prove (memoized per process) that this runtime is in the determinism class whose probe
    /// calibration is `expected` — the hex form of the tag the class computes for
    /// `POW_L1_PALW_PROBE_SEED_V1`.
    fn worker_calibration_once(&self, expected: &'static str) -> Result<(), PowLayer0Error>;
}

static PALW_POW_RUNTIME: OnceLock<&'static dyn PalwPowRuntime> = OnceLock::new();

/// Fill the process's PALW PoW runtime slot. Set-once: the first caller wins and `true` says the
/// call was the one that set it. A second registration is refused rather than swapped — the tag
/// caches, probe memos and every already-computed verdict in this process assume ONE runtime for
/// its lifetime, so late replacement is exactly the kind of half-flip ADR-0042 exists to remove.
pub fn register_palw_pow_runtime(runtime: &'static dyn PalwPowRuntime) -> bool {
    PALW_POW_RUNTIME.set(runtime).is_ok()
}

/// The registered runtime, or the [`PowLayer0Error::PalwUnavailable`] that prices an
/// inference-tagged header as failed PoW on a node that carries no model.
fn runtime() -> Result<&'static dyn PalwPowRuntime, PowLayer0Error> {
    PALW_POW_RUNTIME.get().copied().ok_or_else(|| {
        PowLayer0Error::PalwUnavailable(
            "no PALW model runtime is registered in this process, so inference-priced tags (algo 4/5) cannot be \
             computed and their headers fail PoW. A node or miner on a network that demands them registers the \
             driver at startup (misaka_palw_pow_driver::install()); on devnet, MISAKA_PALW_POW_FIXTURE=1 selects \
             the model-free fixture family instead."
                .into(),
        )
    })
}

/// The PALW Layer-1 tag for one (header, nonce) attempt. Deterministic across every conforming
/// node: fixture nodes derive it from the seed alone; real nodes replay the pinned inference
/// through the registered runtime, which caches by seed — so the header pipeline, block-level
/// derivation and pruning-proof path pay for a given attempt's inference once per process.
pub fn palw_l1_tag(
    pre_pow_hash: Hash64,
    timestamp: u64,
    nonce: u64,
    network_id: &[u8],
) -> Result<[u8; POW_L1_PALW_OUT_BYTES], PowLayer0Error> {
    let seed = palw_pow_seed_v1(pre_pow_hash, timestamp, nonce, network_id);
    // Class-pinned networks verify the runtime ONCE per process, in the path every consumer —
    // node validation, miner, pruning proof — must pass through, so nothing can skip it. This
    // runs BEFORE the fixture branch on purpose: the fixture tag family must fail a class-pinned
    // net loudly instead of minting tags no real peer accepts (class-less nets return instantly).
    verify_worker_calibration(network_id)?;
    if fixture_enabled_for(network_id) {
        return Ok(palw_fixture_l1_tag_v1(&seed));
    }
    runtime()?.tag_for_seed(&seed)
}

/// Verify (once per process, memoized — failure too) that this host's registered runtime is in
/// the determinism class `network_id` pins, by replaying the canonical probe seed through the
/// ordinary tag path and comparing the 200-byte tag against the network's pinned calibration.
/// Networks that pin no class (devnet) pass trivially. Without it, an out-of-class runtime
/// starts happily and then silently forks — rejecting every honest block, having its own
/// rejected — with nothing pointing at the cause. Called eagerly by the kaspad startup rail
/// (good message, before any peer is dialed) and lazily by [`palw_l1_tag`] (so no consumer can
/// skip it). Costs one inference on first use; the probe tag lands in the driver's seed cache.
pub fn verify_worker_calibration(network_id: &[u8]) -> Result<(), PowLayer0Error> {
    let Some(expected) = palw_worker_calibration_v1(network_id) else {
        return Ok(());
    };
    if fixture_requested() {
        // The REQUEST, not its effect. `fixture_permitted_on` already denies this network, so the
        // tag path would compute real tags and this node could join with a real worker — but a
        // class-pinned network is a live one, and a stray fixture variable pointed at it is a
        // misconfiguration worth stopping for rather than quietly overriding. The message names
        // the variable, which is what the operator has to remove.
        //
        // The fixture is its own (model-free) tag family, permitted on devnet-class nets only —
        // and those pin no class. A class-pinned net running the fixture must fail the probe
        // loudly rather than mint fixture tags no real peer accepts.
        return Err(PowLayer0Error::PalwUnavailable(
            "this network pins a worker determinism class; the MISAKA_PALW_POW_FIXTURE tag family cannot join it".into(),
        ));
    }
    runtime()?.worker_calibration_once(expected)
}

/// The PALW-Ollama (`algo_id = 5`) Layer-1 tag for one (header, nonce) attempt. Same seed and
/// canonical prompt as algo 4; the inference runs on the host-local Ollama server behind the
/// registered runtime and the tag commits to the greedy response bytes + token counts. Cached by
/// seed like the worker tag.
pub fn palw_ollama_l1_tag(
    pre_pow_hash: Hash64,
    timestamp: u64,
    nonce: u64,
    network_id: &[u8],
) -> Result<[u8; POW_L1_PALW_OLLAMA_OUT_BYTES], PowLayer0Error> {
    let seed = palw_pow_seed_v1(pre_pow_hash, timestamp, nonce, network_id);
    if fixture_enabled_for(network_id) {
        return Ok(palw_ollama_fixture_l1_tag_v1(&seed));
    }
    runtime()?.ollama_tag_for_seed(&seed)
}
