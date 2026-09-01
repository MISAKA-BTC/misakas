//! kaspa-pq Phase 8 (PR-8.3): Layer 0 PoW finalizer + difficulty-lift
//! helpers.
//!
//! See [ADR-0007](../../docs/adr/0007-layered-pow.md). This module
//! contains the **consensus-critical, frozen** half of the Layered
//! PoW:
//!
//! 1. The BLAKE2b-512 keyed finalizer with
//!    [`POW_FINALIZER_DOMAIN`] as the key
//!    (`b"kaspa-pq-pow-v1"`).
//! 2. The 512-bit comparison domain, exposed as
//!    `Uint512`/`Uint576` operations re-exported from
//!    `kaspa_math`.
//! 3. The difficulty-lift helper that maps an upstream 256-bit
//!    target into the kaspa-pq 512-bit comparison domain
//!    (`target_512 = target_256 << 256`; see the ADR for the
//!    block-finding-probability preservation proof).
//!
//! The module is intentionally self-contained: it does **not**
//! reach into the consensus PoW validator yet. The wiring step
//! (PR-8.6) plugs `pow_finalizer_blake2b_512` into the actual
//! `verify_pow` path and consumes `header.pow_algo_id`
//! (introduced in PR-8.4).
//!
//! `algo_id` semantics: the Layer-1 tag is selected by
//! `header.pow_algo_id`. Defined ids:
//!   - `1` [`POW_ALGO_ID_KHEAVYHASH`] — Phase 1 kHeavyHash matrix.
//!   - `2` [`POW_ALGO_ID_ARGON2ID`] — Phase 2 memory-hard Argon2id
//!     (superseded; still *verifiable* for historical pruning
//!     proofs, but no live network selects it).
//!   - `3` [`POW_ALGO_ID_BLAKE2B_SHA3`] — Phase 3 compute-only
//!     BLAKE2b-512 ∥ SHA3-512 (the active testnet/mainnet algo).
//!   - `4` [`POW_ALGO_ID_PALW_LLM`] — Phase 4 PALW deterministic
//!     pinned-LLM inference (the active devnet algo; the mining
//!     work is one greedy Qwen3.5-2B transcript per attempt).
//! There is **no** mixed-`algo_id` difficulty arithmetic;
//! transitions are hard cut-offs at a specific DAA score, and a
//! header must declare exactly the id its network mandates
//! ([`required_algo_id`] / [`check_algo_id`]).

use blake2b_simd::Params;
use kaspa_hashes::{Hash, Hash64};
use kaspa_math::{Uint256, Uint512, Uint576};
use sha3::{Digest, Sha3_512};

/// BLAKE2b key for the Layer 0 PoW finalizer. Matches the
/// existing `crypto/hashes/src/hashers.rs` pattern of using a
/// short ASCII domain tag as the BLAKE2b key for cross-context
/// hash separation.
pub const POW_FINALIZER_DOMAIN: &[u8] = b"kaspa-pq-pow-v1";

/// Output width of the Layer 0 finalizer in bytes. Compared
/// against a 512-bit (`Uint512`) target.
pub const POW_FINALIZER_BYTES: usize = 64;

/// kaspa-pq Phase 1 Layer 1 algorithm id (the only one valid in
/// Phase 1).
///
/// Semantically: "this header's L1 tag is the upstream
/// `cSHAKE256("HeavyHash")` 32-byte digest, unchanged". Future
/// `algo_id` values introduce ASIC-hard L1 variants and ship in
/// their own hard-fork ADRs.
pub const POW_ALGO_ID_KHEAVYHASH: u8 = 1;

/// Maximum byte length of an L1 tag accepted by the Layer 0
/// finalizer. Acts as a defensive upper bound so a future
/// `algo_id` cannot accidentally inflate header validation cost
/// past a reasonable budget — actual lengths are fixed per
/// `algo_id` and validated up-stack.
pub const POW_L1_TAG_MAX_BYTES: usize = 256;

/// Domain-separator key for the algo_id = 1 (kHeavyHash) seed
/// derivation. kaspa-pq Phase 9 (PR-9.3) — see ADR-0008
/// §"algo_id = 1 (kHeavyHash) seed derivation".
///
/// The upstream kHeavyHash signature takes a 32-byte seed; the
/// kaspa-pq Phase 1 path derives that seed from the 64-byte
/// pre-PoW hash via a dedicated keyed BLAKE2b-256 so the 32-byte
/// seed cannot be substituted for any other 32-byte digest in the
/// system.
pub const POW_L1_KHEAVYHASH_V1_SEED_DOMAIN: &[u8] = b"kaspa-pq-l1-kheavyhash-v1-seed";

/// kaspa-pq Phase 2 Layer 1 algorithm id: **memory-hard Argon2id** (ADR-0007 §"Phase 2").
/// Replaces kHeavyHash on the networks where it is activated (testnet/mainnet) to compress the
/// GPU↔ASIC performance gap and prevent kHeavyHash/BLAKE2b ASICs (incl. Kaspa's) from being
/// reused against this chain. The Layer 0 BLAKE2b-512 finalizer is unchanged; only the Layer 1
/// tag computation differs.
pub const POW_ALGO_ID_ARGON2ID: u8 = 2;

/// Argon2id Layer-1 parameters (`algo_id = 2`). Memory cost is the ASIC-resistance lever; it is
/// paid per *hash attempt* by miners (millions/s → memory-bandwidth bound), while a verifier runs
/// it exactly once per block header (negligible). 16 MiB, 1 pass, 1 lane, 32-byte tag.
pub const POW_L1_ARGON2ID_M_COST_KIB: u32 = 16 * 1024;
pub const POW_L1_ARGON2ID_T_COST: u32 = 1;
pub const POW_L1_ARGON2ID_P_COST: u32 = 1;
pub const POW_L1_ARGON2ID_OUT_BYTES: usize = 32;
/// Domain separator (BLAKE2b key) for the algo_id = 2 Argon2id password + salt derivation.
pub const POW_L1_ARGON2ID_V1_DOMAIN: &[u8] = b"kaspa-pq-l1-argon2id-v1";

/// MISAKA Phase 4 Layer 1 algorithm id: **PALW LLM inference** ("Proof of
/// Artificial-LLM Work"; see docs/adr/0021-palw-llm-pow.md and the
/// Open-then-Audit paper).
///
/// The Layer-1 tag is the replay-stable projection of ONE deterministic
/// pinned-LLM inference (`misaka-palw-worker`: greedy argmax on the pinned
/// Qwen3.5-2B GGUF, `gemm_trace_root` chaining the full logits vector of every
/// decode call) whose prompt is derived from
/// [`palw_pow_seed_v1`]`(network_id, pre_pow_hash, timestamp, nonce)`. Nothing
/// short of running the pinned model reproduces the tag, so the mining "work"
/// IS the inference; verification is the paper's small-`q` full replay (the
/// worker's `verify` mode is `self-job` recomputed).
///
/// **Grinding closure** (this is why the seed takes `timestamp` even though
/// the Layer-0 finalizer already binds it): for the cheap-tag algos (1/2/3)
/// re-hashing the finalizer over a grindable input costs the same as a fresh
/// attempt, so binding `timestamp`/`nonce` only at the finalizer is fine. For
/// PALW the tag is ~10^9× more expensive than the finalizer, so any header
/// input that is miner-adjustable WITHOUT changing the tag becomes a free
/// hash-grinding dimension that collapses the PoW back to BLAKE2b. The two
/// miner-grindable inputs zeroed out of `pre_pow_hash` are exactly `nonce`
/// and `timestamp`; the seed therefore binds both, leaving no header degree
/// of freedom outside the inference.
pub const POW_ALGO_ID_PALW_LLM: u8 = 4;

/// Output width of the `algo_id = 4` PALW Layer-1 tag:
/// `output_commitment (64) ∥ gemm_trace_root (64) ∥
/// operation_schedule_commitment (64) ∥ prefill_tokens (4, LE) ∥
/// decode_tokens (4, LE)` = 200 bytes. Within [`POW_L1_TAG_MAX_BYTES`].
pub const POW_L1_PALW_OUT_BYTES: usize = 200;

/// Domain separator for every PALW-PoW v1 derivation (seed, prompt frame,
/// fixture tag).
pub const POW_L1_PALW_V1_DOMAIN: &[u8] = b"misaka-l1-palw-llm-v1";

/// The `--n-predict` ceiling handed to the PALW worker for every PoW
/// inference. A **frozen consensus constant** of the v1 algorithm (NOT a
/// tunable `Params` field): the worker treats it as the total token budget
/// (`decode budget = n_predict - prefill_tokens`), so two nodes disagreeing
/// here compute different tags for the same header. Changing it is a
/// hard fork = a new algo id, exactly like the other Layer-1 parameters.
pub const POW_L1_PALW_N_PREDICT_V1: u32 = 128;

/// The canonical **calibration probe** seed for `algo_id = 4`: a raw 32-byte PoW seed run through
/// the ordinary worker path (same prompt frame, same frozen `--n-predict`), so the probe measures
/// exactly what block validation will do. Provenance: `BLAKE2b-256("palw-audit-2026-08-16/uniform-0/0")`
/// — the "uniform/u0" seed of the 61-seed forgery audit, whose tag was measured byte-identical on
/// every fleet host (docs/palw-algo4-crosshost-determinism-2026-08-16.md).
pub const POW_L1_PALW_PROBE_SEED_V1: [u8; 32] = [
    0xf5, 0xfe, 0xda, 0x2e, 0xe8, 0xc6, 0xcc, 0x2c, 0xa2, 0x3b, 0x79, 0x6d, 0x48, 0x00, 0xb8, 0xe0, 0x22, 0xcd, 0x89, 0x6f, 0xb2,
    0x95, 0xd5, 0xcb, 0xd2, 0x53, 0x66, 0xaf, 0x8d, 0x4a, 0x19, 0x0e,
];

/// The 200-byte tag [`POW_L1_PALW_PROBE_SEED_V1`] MUST produce on **testnet-11** — the public
/// PALW net's determinism class, pinned (ADR-0035).
///
/// Same rationale as [`POW_L1_PALW_OLLAMA_CALIBRATION_V1`]: the GGUF pin catches the wrong model;
/// this catches everything else that decides the arithmetic (worker build profile, CPU
/// architecture, runtime scheduling). A runtime outside the class would compute a different tag
/// for every header and silently fork itself off the network — so it must refuse to start and
/// say which value it produced instead. The cost is one inference per process start.
///
/// Measured 2026-08-16 on all four fleet hosts — Intel Broadwell + 3× AMD EPYC, four kernel
/// builds, two vendors — byte-identical on each (gate 2: 305/305 tag fields, canonical digest
/// `311d7eab…`). An Apple-Silicon/Metal worker produces a different value and is — correctly —
/// refused: it is not in this network's class (its own nets, e.g. devnet, pin no class here).
pub const POW_L1_PALW_WORKER_CALIBRATION_TN11_V1: &str = "7d1981298652ca5c8fd224dfb6ea8d00787035a0430728d27aa3dd209b38731cc0f5e1cce6ab2a1be8cff97412d4553e0aa512cfc535220cfb57a71a27d060a046f5fc18c9e6564aaa0bc3fd0853802f66a27dfb9647736caa5f91de2ead9cf945d0b7a7d1b81b95ab858b33f260ac907a07f92bb46ae974c37193f4ea1b652ebfbfaad8aa1e587d1d28ad827cca24401c8b6f339a928ea85ea009249976ece4ba684f34d5a6cf911e9d24a9aaed53686c2b16479f1f553b022056d0039375934700000039000000";

/// The pinned worker-class calibration for `network_id` (the `NetworkId::to_string()` bytes the
/// whole Layer-0 path uses), or `None` where no single class is pinned: devnet deliberately pins
/// nothing (any conforming runtime may mine its own mesh — that is what a dev net is for), and
/// nets where algo 4 is inert never reach the check. Every PUBLIC PALW-4 network must add a row
/// here before its activation flips — a class-less public net cannot tell an honest node from a
/// silently-forking one.
pub fn palw_worker_calibration_v1(network_id: &[u8]) -> Option<&'static str> {
    match network_id {
        b"testnet-11" => Some(POW_L1_PALW_WORKER_CALIBRATION_TN11_V1),
        _ => None,
    }
}

/// MISAKA Phase 4b Layer 1 algorithm id: **PALW LLM inference via an Ollama runtime**
/// (ADR-0021 addendum). Same seed, same canonical prompt, same grinding closure as
/// [`POW_ALGO_ID_PALW_LLM`]; the difference is WHERE the inference runs and WHAT the tag can
/// bind:
///
/// * The runtime is a host-local **Ollama server** (`/api/generate`, `raw` mode, temperature 0,
///   [`POW_L1_PALW_OLLAMA_NUM_PREDICT_V1`] new tokens) running the pinned Qwen model — the
///   runtime an Ubuntu VPS fleet can actually operate, where the pinned-llama.cpp Metal worker
///   cannot run at all.
/// * Ollama's API does not expose per-decode logits, so the `gemm_trace_root` binding of the
///   worker tag is UNAVAILABLE: the v1 tag here commits to the greedy RESPONSE BYTES and the
///   token counts only. Weaker binding than algo 4 (an attacker reproducing the exact greedy
///   continuation by other means is not distinguished), still model-work-priced: the response
///   to a fresh 256-bit seed prompt is not predictable without running the model.
/// * Determinism scope is the **runtime class** an operator deploys: same Ollama build, same
///   model digest, same architecture. Greedy decoding across machines of one class is
///   reproducible; across architectures (NEON vs AVX2 reduction order) it is NOT promised —
///   the same arch-scoping the CPU compute class documents. One network = one class.
pub const POW_ALGO_ID_PALW_OLLAMA: u8 = 5;

/// ADR-0042 Decision 3d: the V2 committed-attempt PoW.
///
/// Distinct from `POW_ALGO_ID_PALW_LLM` (4) and `POW_ALGO_ID_PALW_OLLAMA` (5) **so no pre-V2 node
/// can mistake a V2 block for a legacy algo-4 one.** Their L1 tags mean opposite things: algo-4's
/// is the inference itself, V2's is an expansion of the commitment root, and a node applying the
/// wrong reading would accept a block whose work it never checked.
///
/// Reusing 4 and changing its meaning was the alternative, and it is worse for the reason a new
/// genesis makes cheap to avoid: the RC starts from its own genesis, so nothing is owed a
/// compatibility shim, and an id that means two things across a fork is a permanent hazard for a
/// saving of one byte.
pub const POW_ALGO_ID_PALW_COMMITTED_V2: u8 = 6;

/// ADR-0044 Decision 6: the free-prompt **receipt-spend** block.
///
/// Distinct from [`POW_ALGO_ID_PALW_COMMITTED_V2`] (6) because their L1 tags mean different
/// things — 6's is `Expand(commitment_root)` of a fresh chain-challenge attempt (one new ticket
/// costs one new inference), 7's is `Expand(spend_id)` of a **certified** free-prompt quantum
/// (the inference already happened, was audited through the claim lattice, and reached `Final`;
/// the block admission is hashes, signatures, a beacon fact and a set lookup — no model, ever).
/// An id meaning two things across block kinds is the fork hazard a byte of id space is cheap
/// to avoid.
///
/// Like 6 before PR-10: **known, and demanded or accepted by nothing.** No fork flag returns it,
/// [`required_algo_id_for_mode`] never yields it (a `ConsensusV2` network demands 6 exclusively
/// until the FP bundle's wiring swaps the seam to the two-id set), and it is deliberately absent
/// from [`check_algo_id_known`] — the pruning-proof path must not accept a header whose tag this
/// binary cannot yet derive, because "accepted but unverifiable" downstream of that gate is the
/// remote-crash shape the unknown-algo-id P0 already exhibited once.
pub const POW_ALGO_ID_PALW_RECEIPT_V3: u8 = 7;

/// **Does a header of this algorithm buy position in the chain?**
///
/// A receipt header's digest is free to re-roll: nothing in it costs anything to produce, so any
/// position it buys is bought with a signature. Two things are position — the pruning-proof
/// hierarchy (block LEVEL) and the fork-choice weight (blue WORK) — and both must answer no, for
/// the same reason. They used to answer separately, and only the level half said no; a receipt
/// block still added `calc_work(bits)` to its descendants' blue work, which is reorg weight minted
/// out of signatures.
///
/// The lane's real meter is the quantum ticket (ADR-0044 Decision 6), which draws against a beacon
/// derived from the candidate's own chain — so it can only run on a chain candidate and cannot gate
/// DAG entry at all. A merged-but-never-candidate receipt block never faces it, which is why the
/// answer here may not be "the ticket handles it".
///
/// **The heartbeat lane is deliberately NOT here, and the distinction is load-bearing.** It answers
/// no to one of the two purchases and yes to the other, so it needs its own predicate — see
/// [`algo_id_derives_no_block_level`].
#[inline]
pub fn algo_id_carries_no_chain_position(algo_id: u8) -> bool {
    algo_id == POW_ALGO_ID_PALW_RECEIPT_V3
}

/// **Block LEVEL only** — the pruning-proof hierarchy, without the fork-choice half.
///
/// [`algo_id_carries_no_chain_position`] answers both questions at once because for the receipt
/// lane both answers are the same. For the heartbeat lane they are NOT, and reading one predicate
/// for both is a bug I made and this comment exists to stop:
///
/// * **Level: zero.** A heartbeat's target is a network constant, so its digest is not priced by
///   anything that scales — a lucky solve lands far under a FIXED target as often as under a hard
///   one, and `calc_level_from_pow_512` would read that luck as hierarchy. A lane meant to keep a
///   stopped chain ticking must not also buy pruning-proof structure at 2²⁴ hashes a go.
/// * **Blue work: ε, not zero.** Zero is right for the receipt lane because all chain weight comes
///   from the attempt lane. It is WRONG here: the regime this lane exists for is total bonded
///   collapse, where every block is a heartbeat, and with zero work every such branch ties at the
///   selected parent's weight and nothing orders them. `ε × n` orders the longer chain first,
///   which is the whole of ADR-0060 Decision 1.2.
///
/// Folding the heartbeat into the shared predicate makes the ghostdag arm return 0 before the ε
/// arm is ever reached — the two arms are in that order — so the lane silently stops weighing
/// anything and a collapsed chain cannot pick a tip.
#[inline]
pub fn algo_id_derives_no_block_level(algo_id: u8) -> bool {
    algo_id_carries_no_chain_position(algo_id) || algo_id == POW_ALGO_ID_HEARTBEAT_V1
}

/// Output width of the `algo_id = 5` tag:
/// `response_digest (64) ∥ prompt_eval_count (4, LE) ∥ eval_count (4, LE)` = 72 bytes.
pub const POW_L1_PALW_OLLAMA_OUT_BYTES: usize = 72;

/// Domain separator for every PALW-Ollama v1 derivation (tag digest, fixture).
pub const POW_L1_PALW_OLLAMA_V1_DOMAIN: &[u8] = b"misaka-l1-palw-ollama-v1";

/// `options.num_predict` for every PoW inference: the number of NEW tokens Ollama may decode
/// (its `num_predict` is decode-only, unlike the worker's total ceiling). A frozen consensus
/// constant of the v1 algorithm, exactly like [`POW_L1_PALW_N_PREDICT_V1`].
///
/// **16, chosen with the 120 s block interval (2026-08-12).** This constant IS the per-header
/// verification cost every validator pays forever, so it is a capacity parameter, not a quality
/// one: 48 tokens measured ~26-60 s of replay on the slowest fleet host, 16 measures ~12-26 s.
/// Shrinking it does not weaken the proof — the work a miner must do is set by the DIFFICULTY,
/// which the DAA re-derives for whatever an attempt costs; what the token count sets is the
/// floor cost of *checking* someone else's answer, and that floor is what decides whether a
/// modest host can stay on the network. Fewer decode steps also mean fewer near-tie argmax
/// draws, i.e. a slightly wider cross-implementation determinism margin.
pub const POW_L1_PALW_OLLAMA_NUM_PREDICT_V1: u32 = 16;

/// `options.num_gpu` for every PoW inference: **0 — compute on the CPU backend, always.**
///
/// Measured 2026-08-11, and the reason this is a consensus constant rather than a host choice:
/// the same model, same prompt, same options produced DIFFERENT greedy continuations on Ollama's
/// Metal (GPU) backend and its CPU backend. GPU-vs-CPU is therefore a determinism-class
/// dimension, and unlike the Ollama version or the host architecture it is one the protocol
/// *controls* — so it is pinned rather than left to whatever hardware a host happens to have.
/// A fleet mixing GPU-equipped and CPU-only hosts would otherwise split silently, and a
/// GPU-equipped developer box would "verify" a configuration the CPU-only fleet never runs.
///
/// The cost is deliberate: PoW cannot exploit a GPU. That is the same trade the VLT portable CPU
/// profile makes — a class a heterogeneous fleet can actually audit within beats a faster class
/// only some hosts can join.
///
/// Thread count is NOT pinned: measured invariant across `num_thread` 1/4/8 on the CPU backend
/// (ggml sums each dot product within one thread's row chunk, so the reduction order does not
/// move with the split). Leaving it host-chosen lets a bigger VPS use its cores.
pub const POW_L1_PALW_OLLAMA_NUM_GPU_V1: u32 = 0;

/// The canonical **calibration probe** seed for `algo_id = 5`: a fixed 32-byte pattern, run
/// through the ordinary PoW path (same prompt frame, same frozen options) so the probe measures
/// exactly what block validation will do — not an approximation of it.
pub const POW_L1_PALW_OLLAMA_PROBE_SEED_V1: [u8; 32] = [
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44,
    0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
];

/// The Layer-1 tag [`POW_L1_PALW_OLLAMA_PROBE_SEED_V1`] MUST produce, hex-encoded — the network's
/// **determinism class, pinned**.
///
/// The blob pin above catches the wrong model. This catches everything else that decides the
/// arithmetic: the Ollama build, the CPU architecture, any future change in how the runtime
/// schedules the same kernels. Those cannot be pinned individually — pinning an Ollama version
/// would break the fleet on every patch release, and the architecture is a property of the host —
/// but their COMBINED effect is observable in one inference, so that is what is pinned.
///
/// Without this check a node with, say, a newer Ollama starts happily (its blob matches), then
/// computes a different tag for every header: it rejects every honest block, has its own
/// rejected, bans its peers, and the operator sees "invalid PoW" with nothing pointing at the
/// cause. With it, the node refuses to start and says which value it produced. The cost is one
/// inference per process start.
///
/// Measured 2026-08-12 on the x86-64 fleet class (Ollama 0.32.8, `misaka-palw-2b-f16`,
/// AMD EPYC and Intel Broadwell agreeing byte-for-byte). An arm64 machine produces a different
/// value and is therefore — correctly — refused: it is not in this network's class.
pub const POW_L1_PALW_OLLAMA_CALIBRATION_V1: &str = "85afd857dcb8f71ac8a0fdc98f8aace1a4b13a256139424c196a1ed05657b5c0c590c8b93911f5f7c691602411f1702b14d0df3980c6e0ed61ca7ac876b5fefd4400000010000000";

/// The **pinned model blob** for `algo_id = 5`, as Ollama reports it (`GET /api/tags`).
///
/// The weights ARE the algorithm here: a different blob produces different greedy continuations,
/// hence different tags, hence a node that rejects every honest block and whose own blocks are
/// rejected — a silent one-host fork that looks like a network problem. So the digest is pinned
/// in consensus source and verified against the live server before any PoW work is done
/// (`misaka_palw_pow_driver::verify_ollama_model_pin`, called eagerly by the kaspad startup rail
/// and lazily, once per process, by the driver's tag runner — the pin CONSTANT stays here in
/// consensus, the code that reaches a server does not, per ADR-0042 Decision 4). Same stance as
/// the worker's GGUF size+sha check.
///
/// The **F16 profile** of Qwen3.5-2B — `misaka-palw-2b-f16`, created via `ollama create` from
/// the canonical F16 GGUF (sha256 `575eddc35774…`, requantized from unsloth's BF16 export of
/// the base model; NOT the registry's `qwen3.5:2b`, whose Q8_0 blob was measured non-portable).
///
/// Why F16 (measured 2026-08-11→12, 8-seed canonical probe): with the Q8_0 blob the greedy
/// stream diverged between Metal and CPU on one host AND between AMD EPYC and Intel Broadwell
/// within x86-64 (4/8 seeds — quantized dot kernels differ per ISA feature set). With the F16
/// blob every backend runs the f16→fp32-accumulate path and the same eight seeds agree across
/// Metal, arm64 CPU, EPYC and Broadwell — except one seed (3/8) that still splits arm64-vs-x86
/// in the batched prefill GEMM. The class this pins is therefore **x86-64 CPU** (8/8 across
/// vendors); arm64/NVIDIA join only after the 35B PALW runtime's patched-llama.cpp
/// serial-execution policy is ported to the 2B worker (its whole point is closing exactly that
/// residual), or after a probe proves their calibration line equal.
///
/// The OTHER determinism-class dimensions — Ollama version and CPU architecture — deliberately
/// are NOT pinned here: pinning a version would break the fleet on every patch release, and the
/// arch is a property of the host, not of the algorithm. They are operational, covered by the
/// calibration line `scripts/misaka-palw-ollama-setup.sh` prints and compared across the fleet
/// before deployment. Pinning the blob closes the one dimension an operator can get wrong by
/// typing a different model name.
pub const POW_L1_PALW_OLLAMA_MODEL_DIGEST_V1: &str = "d5d0bc552430fc72c69d52583d722a43b8048fa9faf05c2faebabc204f4d13dc";
/// Size in bytes of the pinned blob, checked alongside the digest (cheap defense against a
/// truncated or re-tagged pull).
pub const POW_L1_PALW_OLLAMA_MODEL_SIZE_V1: u64 = 3_775_709_366;

/// kaspa-pq Phase 3 Layer 1 algorithm id: **compute-only BLAKE2b-512 ∥ SHA3-512** (ADR-0007 §"Phase 3").
///
/// Replaces Argon2id (`algo_id = 2`) on the networks where it is activated (testnet/mainnet) to make
/// header verification ~10^4× cheaper, which is the IBD/catch-up bottleneck under a memory-hard PoW
/// (a verifier runs the Layer-1 tag once per header). The trade-off is explicit and accepted: the PoW
/// is no longer memory-hard, so GPU/FPGA/ASIC acceleration is possible — the chain's safety leans on
/// the two-dimensional (PoW × stake) DNS finality overlay (ADR-0009) rather than PoW egalitarianism.
/// The Layer-0 BLAKE2b-512 finalizer is unchanged; only the Layer-1 tag differs.
pub const POW_ALGO_ID_BLAKE2B_SHA3: u8 = 3;

/// **ADR-0066 Decision 1: the heartbeat lane's own algorithm id.**
///
/// The lane used to share `POW_ALGO_ID_BLAKE2B_SHA3` (3) and be told apart by a triple gate —
/// `id == 3 && ConsensusV2 && PALW_HEARTBEAT_LANE_ENABLED`. Three things had to agree for a rule to
/// apply, so a rule could be reached by two of them and missed by the third. Its own id collapses
/// all three into one observable, and buys two more properties outright:
///
/// * a solved algo-3 header from a HASH network can never be replayed as a heartbeat, because the
///   Layer-0 digest binds `pow_algo_id`; and
/// * the id becomes the fence's own observable — `Params::palw_heartbeat` decides whether this id
///   is accepted at all, so "is the lane open" is a params question rather than a `const bool` a
///   rebuild could change without moving any fingerprint.
///
/// **Its Layer-1 tag is algo-3's**, deliberately: the lane is a self-verifying hash lane and wants
/// exactly that tag. Only the id and the target differ, and the id is inside the digest, so the two
/// lanes cannot borrow each other's solutions.
pub const POW_ALGO_ID_HEARTBEAT_V1: u8 = 8;

/// **The heartbeat lane's price, as a work exponent — a network CONSTANT, never `header.bits`.**
///
/// This is the whole of ADR-0066 Decision 1. The first implementation retargeted the lane inside
/// `header.bits`, which is the field the global difficulty window averages: a window of heartbeat
/// rows demanded work 33,554,432, no bonded block could re-enter it, and the chain became
/// heartbeat-only and recoverable only by re-mint. A fixed target cannot feed back on itself.
///
/// 2²⁴ hash evaluations is a couple of seconds of one CPU: a legitimate miner pays it once per
/// interval, a sibling-flooder pays it per block. `Params::palw_heartbeat`'s `work_log2` must equal
/// this or `validate_palw_v2` refuses to start — the fence declares the price it believes it is
/// arming, and a binary that cannot compute that price says so at boot rather than at block one.
pub const PALW_HEARTBEAT_WORK_LOG2: u32 = 24;

/// **How many heartbeat blocks one mergeset may hold — ADR-0066 F3a's bound, as a network
/// CONSTANT.** (ADR-0068 Phase 1.)
///
/// The slot rule bounds the CHAIN (one heartbeat per interval behind its selected parent) and the
/// fixed price bounds nothing but the header rate: siblings share one selected parent, one
/// admissible timestamp and one price, so nothing bounded how many of them the DAG accepts — that
/// is finding 3a, recorded open when the lane landed. The bound that closes it is placed where
/// `mergeset_size_limit` already lives: a valid block's mergeset (selected parent included) may
/// carry at most this many heartbeat-lane members, so sibling floods are absorbed at a bounded
/// rate instead of wholesale, and a flood older than the merge depth is simply never merged.
/// Four is one heartbeat-parent chain slot plus three siblings of healing headroom: a partition
/// that ran on the clock heals a foreign heartbeat chain at three per block, while an honest
/// steady state (one heartbeat per interval) never sees two.
///
/// `Params::palw_heartbeat`'s `max_per_mergeset` must equal this or `validate_palw_v2` refuses to
/// start — same stance, same reason as `work_log2` above.
pub const PALW_HEARTBEAT_MAX_PER_MERGESET: u64 = 4;

/// **The attempt lane's fork-choice work, as a work exponent — a network CONSTANT, never
/// `calc_work(header.bits)`.** ADR-0066 Decision 3 (finding F2), staged there and closed here
/// (ADR-0068 Phase 1).
///
/// On a `ConsensusV2` network the hash target is not the throttle — the class lottery is — so
/// `header.bits` sits at `MAX_DIFFICULTY_TARGET` and `calc_work` prices every bonded block at
/// **2**. Against `HEARTBEAT_BLUE_WORK_EPSILON = 1` that is parity for two sibling heartbeats per
/// layer (~280 kH/s), which is finding F2: ε was never too large, the other side of the
/// comparison was too small. Under the fence, an attempt-lane block's blue work is this constant
/// instead: 2²⁰, so a bonded block outweighs a million heartbeats again and the ε ordering among
/// heartbeat-only branches (total collapse) is undisturbed.
///
/// **A constant, deliberately NOT the envelope's claimed pwu.** The claim is verified against
/// class state (target, per-inference cost) and class state lives on the selected chain — at
/// GHOSTDAG time there is only the header. A claim-derived work would let a shape-valid header
/// that never faces the lottery mint fork-choice weight with a number; a constant keeps the
/// spam/honest ratio exactly where it is today (parity at work 2, parity at work 2²⁰) while
/// fixing the one ratio F2 is about. Per-CLASS weight (a QWEN36 block vs a floor block) is not
/// this layer's job either: that is the pwu-verified PALW chain weight (`safe(C)`, ADR-0058),
/// which only counts what the chain actually checked.
///
/// `Params::palw_attempt_work`'s `work_log2` must equal this or `validate_palw_v2` refuses to
/// start — the fence declares the price it believes it is arming.
pub const PALW_ATTEMPT_BLUE_WORK_LOG2: u32 = 20;

/// Output width of the algo_id = 3 Layer-1 tag: BLAKE2b-512 (64) ∥ SHA3-512 (64) = 128 bytes. Within
/// [`POW_L1_TAG_MAX_BYTES`] (256), so the Layer-0 finalizer accepts it.
pub const POW_L1_BLAKE2B_SHA3_OUT_BYTES: usize = 128;
/// Domain separator for the algo_id = 3 BLAKE2b-512 ∥ SHA3-512 Layer-1 tag. Used as the BLAKE2b key
/// for the first half and as an explicit prefix for the (un-keyed) SHA3-512 second half.
pub const POW_L1_BLAKE2B_SHA3_V1_DOMAIN: &[u8] = b"kaspa-pq-l1-blake2b-sha3-v1";

/// Errors returned by Layer 0 helpers.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PowLayer0Error {
    #[error("kaspa-pq Layer 0: L1 tag length {0} exceeds POW_L1_TAG_MAX_BYTES = {POW_L1_TAG_MAX_BYTES}")]
    L1TagTooLong(usize),
    #[error("kaspa-pq Layer 0: pow_algo_id = {0} is unrecognised or wrong for this network's active PoW phase")]
    UnknownAlgoId(u8),
    /// The PALW (`algo_id = 4`) Layer-1 tag needs the pinned LLM worker and it is not usable in
    /// this process — `PALW_WORKER`/`MISAKA_PALW_GGUF` unset, the binary missing, or a wasm build.
    /// Carries an operator-actionable reason; validating a PALW header without a worker is a node
    /// configuration error, not a bad block.
    #[error("PALW Layer-1 unavailable: {0}")]
    PalwUnavailable(String),
    /// The PALW worker ran but did not produce a usable projection (non-zero exit, timeout, or an
    /// unparseable/short document). Deterministic-compute contract means a *disagreeing* projection
    /// simply yields a tag that fails the target; this variant is for runs that yielded no tag.
    #[error("PALW worker failed: {0}")]
    PalwWorkerFailed(String),
    /// MISAKA ADR-0038: a non-PALW header carries `palw_commitment` bytes. The field is
    /// hash-invisible on non-PALW algo ids (see `hashing::header::write_header_preimage`), so
    /// non-empty bytes there would be block-hash malleability — two distinct serialized blocks
    /// with one identity. Structural refusal, independent of any activation fence.
    #[error("non-PALW header (algo_id = {algo_id}) carries {got} palw_commitment bytes; must be empty")]
    NonPalwHeaderCarriesPalwCommitment { algo_id: u8, got: usize },
    /// MISAKA ADR-0038: a PALW header's `palw_commitment` exceeds the wire cap.
    #[error("palw_commitment is {got} bytes, above the cap {cap}")]
    PalwCommitmentTooLong { got: usize, cap: usize },
    /// MISAKA ADR-0038: a PALW header carries a `palw_commitment` while **nothing in the PoW
    /// path binds it** — the same malleability as
    /// [`Self::NonPalwHeaderCarriesPalwCommitment`], reached from the other side.
    ///
    /// On a PALW header the commitment is *identity*-visible and *PoW*-invisible (it is a
    /// function of the winning nonce, so it cannot sit in the pre-PoW preimage without
    /// circularity). Until some PoW-path digest consumes it, those two facts compose into:
    /// one PoW solution mints unlimited distinct valid block identities. The field is
    /// therefore refused non-empty until the binding exists — see
    /// [`check_palw_commitment_shape`] for the exact precondition to relax this.
    #[error(
        "PALW header (algo_id = {algo_id}) carries {got} palw_commitment bytes, but no PoW-path digest binds them yet; must be empty"
    )]
    PalwCommitmentNotYetBound { algo_id: u8, got: usize },
    /// ADR-0038 Decision A is installed and the header's commitment is not a commitment.
    #[error("PALW header (algo_id = {algo_id}) carries a palw_commitment that is not a well-formed PBC1 commitment: {reason}")]
    PalwCommitmentMalformed { algo_id: u8, reason: String },
    /// ADR-0042 Decision 3a: an algo-6 header reached the finalizer without a decodable V2
    /// attempt envelope in `palw_commitment`. The envelope IS what the committed-V2 PoW prices
    /// — its absence means there is no work to check, so the header cannot be valid. Total
    /// error, never a panic: the pruning-proof path computes PoW on peer-supplied headers
    /// before any shape gate has run.
    #[error("PALW-V2 header (algo_id = 6) carries no decodable attempt envelope; the PoW has nothing to price")]
    PalwV2AttemptMissing,
    /// The algo-7 counterpart (ADR-0044 Decision 6): a V3-lineage header reached the finalizer
    /// without the carriage its tag expands. Same posture as `PalwV2AttemptMissing` — the shape
    /// gate refuses such a header up-stack, so reaching here means a caller skipped it, and a
    /// failed PoW is the answer rather than an `expect` on absent data.
    #[error("PALW header (algo_id = {0}) carries no decodable carriage for its lane's tag")]
    PalwCarriageMissing(u8),
    /// ADR-0042 Decision 3a: the carried attempt's `challenge` is not the one this header
    /// position derives — the envelope was mined for a different (pre_pow_hash, timestamp,
    /// nonce, class, bond). Refused by the finalizer arm itself so that EVERY path that
    /// computes PoW refuses a re-mounted attempt, including proof paths that never reach
    /// stateful admission.
    #[error("PALW-V2 attempt's carried challenge is not the one this header position derives")]
    PalwV2ChallengeMismatch,
}

/// MISAKA ADR-0038: the PALW family of Layer-1 algo ids — the ids whose headers carry (and
/// hash) a `palw_commitment`, and the gate `hashing::header::write_header_preimage` reads.
#[inline]
/// The V2-LINEAGE ids: the attempt lane and the receipt lane. Distinct from
/// [`is_palw_algo_id`], which is the whole PALW family including the V1 lineage — these are the
/// two ids that carry a `PalwChainStateV2`, and the ones the state root's preimage gate reads.
pub fn is_palw_v2_algo_id(algo_id: u8) -> bool {
    algo_id == POW_ALGO_ID_PALW_COMMITTED_V2 || algo_id == POW_ALGO_ID_PALW_RECEIPT_V3
}

pub fn is_palw_algo_id(algo_id: u8) -> bool {
    algo_id == POW_ALGO_ID_PALW_LLM
        || algo_id == POW_ALGO_ID_PALW_OLLAMA
        || algo_id == POW_ALGO_ID_PALW_COMMITTED_V2
        || algo_id == POW_ALGO_ID_PALW_RECEIPT_V3
}

/// MISAKA ADR-0038: wire cap for `Header::palw_commitment` — the PBC1 envelope (4 magic +
/// borsh body incl. one ML-DSA-87 signature ≈ 4.9 KB) with headroom, far below anything that
/// could stress header relay, and small enough that a spam candidate cannot smuggle bulk data.
pub const PALW_COMMITMENT_MAX_BYTES: usize = 8192;

/// MISAKA ADR-0038: structural shape rule for `Header::palw_commitment`, enforced wherever
/// header shape is validated (alongside [`check_algo_id`]) and NOT behind any activation
/// fence:
///
/// * non-PALW `algo_id` → the field MUST be empty. It is hash-invisible there, and a
///   hash-invisible non-empty field is block-hash malleability (two serialized blocks, one
///   identity) — a relay/dedup poison, refused at the door.
/// * PALW V1 `algo_id` (4 / 5) → the field MUST **also** be empty, for now. See below.
/// * `POW_ALGO_ID_PALW_COMMITTED_V2` (6) → the field is REQUIRED, and required to decode as a
///   [`crate::palw_attempt_v2::PalwAttemptEnvelopeV2`]. V2's binding is intrinsic — the finalizer
///   tag is `Expand(commitment_root_v2)` — so the empty-until-bound reasoning below does not
///   apply to it, and neither does the `bound` fence (that fence is V1's rebinding decision).
///
/// # Why the PALW side is empty-only until the PoW binds it
///
/// The first draft of this rule allowed any length up to [`PALW_COMMITMENT_MAX_BYTES`] on a
/// PALW header, reasoning that requiring a commitment is an activation decision rather than a
/// shape rule. That is true of *requiring* one. It is not true of *permitting* one, and the
/// permission was the bug (mainnet-readiness re-audit, 2026-08-17, blocker 4):
///
/// * `hashing::header` writes the commitment into the **block-identity** digest only, and
///   passes `Exclude` on every PoW-path digest — correctly, because the commitment is a
///   function of the winning nonce and cannot enter the pre-PoW preimage without circularity
///   (the algo-4 seed already consumes `pre_pow_hash`).
/// * Nothing in `consensus/pow` reads `palw_commitment` — the PoW path still derives its
///   Layer-1 tag from the inference, not from this field.
///
/// Compose those and the field is identity-visible, PoW-invisible and content-unchecked:
/// **one PoW solution mints unlimited distinct valid block identities**, each a separate DAG
/// entry. That is the same defect the non-PALW arm above refuses, arrived at from the other
/// direction, and it was live and unfenced on every PALW network.
///
/// Empty-only is a stronger containment than an activation fence would be, and deliberately so:
/// a fence is a switch somebody can flip before the binding exists, whereas an unrepresentable
/// state cannot be flipped at all. Honest nodes lose nothing — no mining path populates this
/// field (its only non-test writers are the p2p and RPC *deserializers*, i.e. untrusted peer
/// input), so this rule rejects exactly the attack and nothing else.
///
/// **Precondition to relax this**: some PoW-path digest must consume the commitment, so that
/// altering it invalidates the solution. Under ADR-0038 Decision A that is the ticket rebinding
/// — the Layer-1 tag must be derived from the committed root rather than re-run inference — at
/// which point this arm becomes "must decode as PBC1", the digest gate in `hashing::header`
/// opens with it, and the two land together behind one activation fence. Relaxing it before
/// then re-opens the malleability.
#[inline]
pub fn check_palw_commitment_shape(algo_id: u8, palw_commitment: &[u8], bound: bool) -> Result<(), PowLayer0Error> {
    if !is_palw_algo_id(algo_id) {
        // Unconditional, fence or no fence: a non-PALW header's commitment is hash-INVISIBLE
        // (`write_header_preimage` length-prefixes it only for PALW ids), so a non-empty one is
        // block-hash malleability. This arm never relaxes.
        if !palw_commitment.is_empty() {
            return Err(PowLayer0Error::NonPalwHeaderCarriesPalwCommitment { algo_id, got: palw_commitment.len() });
        }
        return Ok(());
    }
    // The cap is checked first so an oversized payload reports the cap it broke rather than the
    // binding rule — the two errors mean different things to an operator.
    if palw_commitment.len() > PALW_COMMITMENT_MAX_BYTES {
        return Err(PowLayer0Error::PalwCommitmentTooLong { got: palw_commitment.len(), cap: PALW_COMMITMENT_MAX_BYTES });
    }
    // The V2-lineage lanes carry their OWN objects under their own magics, and they carry them
    // ALWAYS — not behind the V1 fence (ADR-0042 Decision 1 removed that machine, and ADR-0044
    // kept its lesson). No fence parameter is consulted here because none is needed: an algo-6
    // or algo-7 header only exists on a network whose mode demands that id, which
    // `check_algo_id_for_mode` decided up-stack. What the finalizer expands is exactly what this
    // gate insists is present and well-formed, so "tagged but unvalidated" is unrepresentable.
    if algo_id == POW_ALGO_ID_PALW_COMMITTED_V2 {
        // The field is REQUIRED and required to be a V2 envelope: an algo-6 header without one
        // carries no work to price, and the finalizer refuses it as `PalwV2AttemptMissing`
        // anyway; failing HERE names the shape defect (wrong magic, truncated body, zero pwu…)
        // instead of a digest mismatch.
        let envelope = crate::palw_attempt_v2::PalwAttemptEnvelopeV2::decode_wire(palw_commitment)
            .map_err(|e| PowLayer0Error::PalwCommitmentMalformed { algo_id, reason: e.to_string() })?;
        return envelope.validate_shape_v2().map_err(|e| PowLayer0Error::PalwCommitmentMalformed { algo_id, reason: e.to_string() });
    }
    if algo_id == POW_ALGO_ID_PALW_RECEIPT_V3 {
        return crate::palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3::decode(palw_commitment)
            .map(|_| ())
            .map_err(|e| PowLayer0Error::PalwCommitmentMalformed { algo_id, reason: e.to_string() });
    }
    if !bound {
        // ADR-0038 Decision A is not installed on this network at this DAA, so the field stays
        // shut. That refusal was never a placeholder: the field IS hash-visible on a PALW header,
        // so bytes nothing validates would let one inference mint distinct blocks.
        if !palw_commitment.is_empty() {
            return Err(PowLayer0Error::PalwCommitmentNotYetBound { algo_id, got: palw_commitment.len() });
        }
        return Ok(());
    }
    // Bound: the commitment is required, and must be a commitment rather than arbitrary bytes.
    // Bound: the commitment is required, and must be a commitment rather than arbitrary bytes.
    // Emptiness falls out of the decoder (no PBC1 magic), which is the right error to report —
    // "this is not a commitment", not "this is the wrong length".
    let commitment = crate::palw_block_commitment::PalwBlockCommitmentV1::decode(palw_commitment)
        .map_err(|e| PowLayer0Error::PalwCommitmentMalformed { algo_id, reason: e.to_string() })?;
    // Shape only. Whether `pwu_claim` equals its derivation from the class's DAA target, and
    // whether `executor_bond_outpoint` names an Active bond, are stateful questions this pure
    // function cannot ask — they belong to `validate_against_class_v1` and to the bond check, at
    // the consumer entry that holds that state. Decision A's other clauses are still unwired.
    commitment.validate_shape().map_err(|e| PowLayer0Error::PalwCommitmentMalformed { algo_id, reason: e.to_string() })
}

/// Validate that an `algo_id` is recognised by this binary at
/// Phase 1. Rejects everything except `POW_ALGO_ID_KHEAVYHASH`.
#[inline]
pub fn check_algo_id_phase1(algo_id: u8) -> Result<(), PowLayer0Error> {
    if algo_id == POW_ALGO_ID_KHEAVYHASH { Ok(()) } else { Err(PowLayer0Error::UnknownAlgoId(algo_id)) }
}

/// **V2 is deliberately absent from this cascade** (ADR-0042 Decision 1). Its activation is not a
/// sixth boolean beside the others — the ADR's whole point is that a ruleset with five independently
/// flippable switches is the defect, since a human flips them in the wrong order. `PalwConsensusMode`
/// carries the V2 ruleset whole or not at all, and the arm that returns
/// [`POW_ALGO_ID_PALW_COMMITTED_V2`] lands with it in PR-10. Until then this function can never
/// return it, which is what makes the id inert on every shipped network while its semantics exist.
///
/// The Layer-1 algorithm a header MUST declare, given which PoW forks are active at the header's
/// DAA score. The PALW-Ollama fork (`algo_id = 5`) supersedes everything where activated; then
/// the PALW worker fork (`algo_id = 4`); then the Phase-3 BLAKE2b-512 ∥ SHA3-512 fork
/// (`algo_id = 3`); otherwise the Phase-1 `algo_id = 1` (kHeavyHash). This is a hard cut-off —
/// there is no mixed-algo arithmetic. (Argon2id, `algo_id = 2`, is the superseded Phase-2
/// algorithm: still *verifiable* via [`check_algo_id_known`] for historical pruning proofs, but
/// no live network selects it.)
#[inline]
pub fn required_algo_id(palw_ollama_active: bool, palw_llm_active: bool, blake2b_sha3_active: bool) -> u8 {
    if palw_ollama_active {
        POW_ALGO_ID_PALW_OLLAMA
    } else if palw_llm_active {
        POW_ALGO_ID_PALW_LLM
    } else if blake2b_sha3_active {
        POW_ALGO_ID_BLAKE2B_SHA3
    } else {
        POW_ALGO_ID_KHEAVYHASH
    }
}

/// Validate a header's `algo_id` against the network's PoW state: it must equal
/// [`required_algo_id`]. Rejects both unknown ids and the *wrong-but-known* id (e.g. a miner trying
/// the cheap kHeavyHash on a BLAKE2b-SHA3 or PALW network, or vice-versa).
#[inline]
pub fn check_algo_id(
    algo_id: u8,
    palw_ollama_active: bool,
    palw_llm_active: bool,
    blake2b_sha3_active: bool,
) -> Result<(), PowLayer0Error> {
    if algo_id == required_algo_id(palw_ollama_active, palw_llm_active, blake2b_sha3_active) {
        Ok(())
    } else {
        Err(PowLayer0Error::UnknownAlgoId(algo_id))
    }
}

/// The algorithm a header MUST declare, with the V2 mode consulted FIRST (ADR-0042 Decision 1,
/// PR-08 seam).
///
/// `mode_required` is [`crate::palw_mode_v2::PalwConsensusMode::required_algo_id`]: `Some(6)`
/// exactly on a `ConsensusV2` network, `None` on `Disabled` / `LegacyTn11` — which is **every**
/// network that exists today. So on every shipped preset this is `required_algo_id(...)`
/// unchanged, byte for byte; only a network carrying the whole atomic V2 bundle demands the
/// committed-V2 id, and it does so exclusively — a V2 network accepts nothing else, and the V1
/// cascade is not consulted for it. This is the one place "does this network demand V2" is
/// decided, so a site cannot answer it a second, drifting way.
#[inline]
pub fn required_algo_id_for_mode(
    mode_required: Option<u8>,
    palw_ollama_active: bool,
    palw_llm_active: bool,
    blake2b_sha3_active: bool,
) -> u8 {
    mode_required.unwrap_or_else(|| required_algo_id(palw_ollama_active, palw_llm_active, blake2b_sha3_active))
}

/// [`check_algo_id`], with the V2 mode consulted first via [`required_algo_id_for_mode`]. The
/// validation processors and the pruning-proof gate call THIS, so the mode's demand and the
/// header's declaration are compared in one place.
#[inline]
pub fn check_algo_id_for_mode(
    algo_id: u8,
    mode_required: Option<u8>,
    palw_ollama_active: bool,
    palw_llm_active: bool,
    blake2b_sha3_active: bool,
) -> Result<(), PowLayer0Error> {
    check_algo_id_for_mode_accepting(algo_id, mode_required, None, palw_ollama_active, palw_llm_active, blake2b_sha3_active)
}

/// [`check_algo_id_for_mode`], asking the network what it ACCEPTS rather than what it demands.
///
/// A V2 bundle admits two lanes — the committed attempt and the free-prompt receipt spend — and a
/// gate comparing against `required_algo_id` alone refused every block on the second. The two
/// questions are genuinely different and both are needed: a PRODUCER building an attempt must
/// declare `required_algo_id`, while a VALIDATOR deciding whether a header may exist must ask
/// `accepts_algo_id`. Conflating them is what left the receipt lane unenterable on a network whose
/// own bundle said it was open.
///
/// `mode_accepts` is [`crate::palw_mode_v2::PalwConsensusMode::accepts_algo_id`]. `None` — every
/// non-V2 preset, and any caller that has not been given the mode — falls through to the exact
/// single-id comparison this function had before, byte for byte.
pub fn check_algo_id_for_mode_accepting(
    algo_id: u8,
    mode_required: Option<u8>,
    mode_accepts: Option<bool>,
    palw_ollama_active: bool,
    palw_llm_active: bool,
    blake2b_sha3_active: bool,
) -> Result<(), PowLayer0Error> {
    if let Some(accepted) = mode_accepts {
        return if accepted { Ok(()) } else { Err(PowLayer0Error::UnknownAlgoId(algo_id)) };
    }
    if algo_id == required_algo_id_for_mode(mode_required, palw_ollama_active, palw_llm_active, blake2b_sha3_active) {
        Ok(())
    } else {
        Err(PowLayer0Error::UnknownAlgoId(algo_id))
    }
}

/// Does Layer-0 PoW verification short-circuit for this header instead of reaching the Layer-1
/// finalizer?
///
/// **The one definition of "parentless root", shared by the PoW entry points and by every gate that
/// must run before them.** `kaspa_pow::calc_block_level_check_pow_layer0` returns
/// `(max_block_level, true)` without touching the finalizer exactly when this is true.
///
/// It is `parents_by_level.is_empty()`, NOT `direct_parents().is_empty()`, and the difference is a
/// live remote-panic vector rather than a nicety. [`crate::header::Header::direct_parents`] reads
/// `parents_by_level[0]` and yields `&[]` when that run exists but is EMPTY — so for a header whose
/// `parents_by_level` is `[[]]`, `direct_parents()` says "parentless" while the PoW short-circuit
/// does not fire. A gate exempting parentless headers on the `direct_parents` predicate therefore
/// skipped `check_algo_id` on precisely the header shape whose PoW still runs, letting
/// `algo_id = 4` reach the PALW arm on a worker-less node and panic it. Found by an adversarial
/// verifier with a proof-of-concept after the first version of the pruning-proof gate shipped with
/// the mismatched predicate (mainnet-readiness audit P0-1, trigger b).
///
/// Callers that mean "the PoW will actually run, so gate the input first" must use this.
#[inline]
pub fn pow_short_circuits_as_parentless_root(header: &crate::header::Header) -> bool {
    header.parents_by_level.is_empty()
}

/// Accept any algo_id this binary knows how to verify ({kHeavyHash, Argon2id, BLAKE2b-SHA3,
/// PALW LLM, PALW Ollama}).
///
/// **NOT USED BY ANY PATH, deliberately.** The doc here used to say "e.g. the pruning-proof path",
/// and that path does NOT use it: `pruning_proof::check_proof_header_shape` applies the strict
/// per-DAA [`check_algo_id`], because proof-only headers below the pruning point are never
/// re-processed by the main pipeline and the looser rule would admit an algo the network did not
/// mandate at that DAA (audit POW-01). Reading this function's doc as a description of live
/// behaviour was therefore wrong in the one direction that matters: it described a defence that
/// was not wired.
///
/// It is kept because the strict rule has a real cost it names correctly: `required_algo_id` never
/// returns Argon2id (2), so a chain that had actually run the Phase-2 algorithm could not have its
/// historical proof headers validated under [`check_algo_id`]. No shipped preset activates
/// Argon2id, so nothing is broken today — but a network that ever does must switch its proof path
/// to a rule that admits the algo its own history used, and this is that rule.
///
/// A caller MUST NOT substitute this for [`check_algo_id`] on a network whose history is
/// single-algo: accepting "any known id" there lets a proof header claim an algorithm the network
/// never mandated, which is a cheaper PoW than the one its difficulty was set for.
/// **This list is the set of ids `kaspa_pow::StateLayer0::calculate_l1_tag` implements.** It is
/// not the set of ids that have a CONSTANT — those are two different things, and conflating them
/// was audit C1: `POW_ALGO_ID_PALW_COMMITTED_V2` had a constant, four pipeline gates that could
/// DEMAND it, and no arm in the finalizer, so a network in `ConsensusV2` mode booted happily,
/// accepted its parentless genesis, and then rejected every block after it — its own miner's
/// included — as `InvalidPoW`, with no fallback id accepted and no pruning proof importable.
/// Listing 6 here said the binary could verify something it cannot; 6 was therefore delisted
/// until the arm existed, and re-listed in the same commit that landed the arm and its carrier
/// (`palw_v2_commitment_mutation_invalidates_pow` is the test that holds the three together).
///
/// Adding an arm to the finalizer means adding its id here, and the mode gate
/// (`PalwConsensusParamsV2::validate`) reads this function to refuse a ruleset whose algorithm
/// this binary cannot compute — so the two can only be wrong together, loudly, at startup.
#[inline]
pub fn check_algo_id_known(algo_id: u8) -> Result<(), PowLayer0Error> {
    if algo_id == POW_ALGO_ID_KHEAVYHASH
        || algo_id == POW_ALGO_ID_ARGON2ID
        || algo_id == POW_ALGO_ID_BLAKE2B_SHA3
        || algo_id == POW_ALGO_ID_PALW_LLM
        || algo_id == POW_ALGO_ID_PALW_OLLAMA
        || algo_id == POW_ALGO_ID_PALW_COMMITTED_V2
        // ADR-0044 Unit B: 7 joins the set the moment its finalizer arm exists. "Known" here
        // means "this binary can derive its tag", and the pruning-proof path is the caller that
        // needs the distinction — a proof header whose tag we cannot derive must be refused, and
        // one whose tag we CAN derive must not be.
        || algo_id == POW_ALGO_ID_PALW_RECEIPT_V3
        // ADR-0066 Decision 1: the heartbeat lane. Known unconditionally — "this binary can derive
        // the tag" is a statement about the binary, not about whether any network accepts the id.
        // Whether a header carrying it is VALID is `Params::palw_heartbeat`'s question, asked in
        // `check_pow_algo_id`; conflating the two is what the triple gate did.
        || algo_id == POW_ALGO_ID_HEARTBEAT_V1
    {
        Ok(())
    } else {
        Err(PowLayer0Error::UnknownAlgoId(algo_id))
    }
}

/// kaspa-pq Layer 0 PoW finalizer.
///
/// Layout (ADR-0007 §"Decision", ADR-0008-updated to take a
/// 64-byte `pre_pow_hash`):
///
/// ```text
/// pow_512 = BLAKE2b-512(
///     key   = POW_FINALIZER_DOMAIN,
///     input = network_id_len_le_u16 || network_id ||
///             algo_id ||
///             pre_pow_hash64 ||                     // 64 bytes
///             timestamp.to_le_bytes() ||
///             bits.to_le_bytes() ||
///             nonce.to_le_bytes() ||
///             (l1_tag.len() as u16).to_le_bytes() || l1_tag,
/// )
/// ```
///
/// All variable-length inputs (`network_id`, `l1_tag`) carry a
/// 2-byte little-endian length prefix in front so the input is
/// self-delimiting: adding a new `algo_id` whose tag is a
/// different length cannot collide with a previous variant's
/// concatenation.
///
/// Returns the 64-byte digest. The caller compares against the
/// 512-bit target via `Uint512::from_le_bytes` /
/// `Uint512::from_compact_target_bits_512`.
pub fn pow_finalizer_blake2b_512(
    network_id: &[u8],
    algo_id: u8,
    pre_pow_hash: Hash64,
    timestamp: u64,
    bits: u32,
    nonce: u64,
    l1_tag: &[u8],
) -> Result<[u8; POW_FINALIZER_BYTES], PowLayer0Error> {
    if l1_tag.len() > POW_L1_TAG_MAX_BYTES {
        return Err(PowLayer0Error::L1TagTooLong(l1_tag.len()));
    }

    let mut state = Params::new().hash_length(POW_FINALIZER_BYTES).key(POW_FINALIZER_DOMAIN).to_state();

    // 2-byte length-prefix for the variable-width network_id so the
    // domain separation is unambiguous across simnet / devnet /
    // testnet / mainnet, which all carry distinct network_id bytes
    // (see ADR-0001).
    state.update(&(network_id.len() as u16).to_le_bytes());
    state.update(network_id);

    state.update(&[algo_id]);
    // ADR-0008: pre_pow_hash is now 64 bytes (BlockPrePowHash64).
    state.update(&pre_pow_hash.as_bytes());
    state.update(&timestamp.to_le_bytes());
    state.update(&bits.to_le_bytes());
    state.update(&nonce.to_le_bytes());

    state.update(&(l1_tag.len() as u16).to_le_bytes());
    state.update(l1_tag);

    let digest = state.finalize();
    let mut out = [0u8; POW_FINALIZER_BYTES];
    out.copy_from_slice(digest.as_bytes());
    Ok(out)
}

/// Derive the 32-byte kHeavyHash v1 seed from the 64-byte
/// pre-PoW hash. kaspa-pq Phase 9 (PR-9.3); see ADR-0008
/// §"algo_id = 1 (kHeavyHash) seed derivation".
///
/// ```text
/// l1_seed32 = BLAKE2b-256(
///     key   = POW_L1_KHEAVYHASH_V1_SEED_DOMAIN,
///     input = pre_pow_hash64,
/// )
/// ```
///
/// This bridges the 64-byte Layer 0 pre-PoW hash to the upstream
/// 32-byte kHeavyHash interface for the Phase 1 `algo_id = 1`
/// path. The seed is domain-separated on its own keyed BLAKE2b
/// instance so the 32-byte seed and the 64-byte pre-PoW hash
/// cannot be substituted for each other anywhere else.
#[inline]
pub fn l1_seed32_for_kheavyhash_v1(pre_pow_hash: Hash64) -> Hash {
    let digest =
        Params::new().hash_length(32).key(POW_L1_KHEAVYHASH_V1_SEED_DOMAIN).to_state().update(pre_pow_hash.as_byte_slice()).finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_bytes());
    Hash::from_bytes(out)
}

/// kaspa-pq Phase 2 (`algo_id = 2`): the memory-hard Argon2id Layer-1 tag.
///
/// ```text
/// password = BLAKE2b-256(key = POW_L1_ARGON2ID_V1_DOMAIN, pre_pow_hash64 || nonce_le)
/// salt     = BLAKE2b-128(key = POW_L1_ARGON2ID_V1_DOMAIN, "salt" || netid_len_le || network_id)
/// l1_tag   = Argon2id(password, salt; m=16MiB, t=1, p=1, out=32)
/// ```
///
/// Deterministic (fixed params + fixed per-network salt), binds to the block via `pre_pow_hash`
/// and to the search via `nonce`, and is domain/network-separated. The 32-byte tag is then fed to
/// the unchanged Layer-0 `pow_finalizer_blake2b_512` with `algo_id = 2`.
pub fn argon2id_l1_tag_v1(pre_pow_hash: Hash64, nonce: u64, network_id: &[u8]) -> [u8; POW_L1_ARGON2ID_OUT_BYTES] {
    // password: per-(block, nonce) — this is what miners vary across nonce trials.
    let password = {
        let digest = Params::new()
            .hash_length(32)
            .key(POW_L1_ARGON2ID_V1_DOMAIN)
            .to_state()
            .update(pre_pow_hash.as_byte_slice())
            .update(&nonce.to_le_bytes())
            .finalize();
        let mut o = [0u8; 32];
        o.copy_from_slice(digest.as_bytes());
        o
    };
    // salt: fixed per network (deterministic). Length-prefixed network id for unambiguous separation.
    let salt = {
        let digest = Params::new()
            .hash_length(16)
            .key(POW_L1_ARGON2ID_V1_DOMAIN)
            .to_state()
            .update(b"salt")
            .update(&(network_id.len() as u16).to_le_bytes())
            .update(network_id)
            .finalize();
        let mut o = [0u8; 16];
        o.copy_from_slice(digest.as_bytes());
        o
    };
    let params = argon2::Params::new(
        POW_L1_ARGON2ID_M_COST_KIB,
        POW_L1_ARGON2ID_T_COST,
        POW_L1_ARGON2ID_P_COST,
        Some(POW_L1_ARGON2ID_OUT_BYTES),
    )
    .expect("static Argon2id params are valid");
    let a2 = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut out = [0u8; POW_L1_ARGON2ID_OUT_BYTES];
    a2.hash_password_into(&password, &salt, &mut out).expect("Argon2id hash into fixed-size buffer");
    out
}

/// kaspa-pq Phase 3 (`algo_id = 3`): the compute-only BLAKE2b-512 ∥ SHA3-512 Layer-1 tag.
///
/// ```text
/// half_b = BLAKE2b-512(key = DOMAIN, netid_len_le16 || network_id || pre_pow_hash64 || nonce_le)
/// half_s = SHA3-512(DOMAIN || netid_len_le16 || network_id || pre_pow_hash64 || nonce_le)
/// l1_tag = half_b || half_s                                          // 64 + 64 = 128 bytes
/// ```
///
/// Both halves bind the block (`pre_pow_hash`), the search (`nonce`), and the network (length-prefixed
/// `network_id`), and are domain-separated on `DOMAIN` — the BLAKE2b half uses it as the key, the
/// (un-keyed) SHA3 half prepends it. The 128-byte tag is fed to the unchanged Layer-0
/// `pow_finalizer_blake2b_512` with `algo_id = 3`, which mixes a *second* BLAKE2b-512 over the whole
/// preimage (including `half_s`) — so a miner cannot skip the SHA3 half: the accepted digest depends
/// on every tag byte. Per-nonce work is therefore 2×BLAKE2b-512 + 1×SHA3-512, all compute-only.
pub fn blake2b_sha3_l1_tag_v1(pre_pow_hash: Hash64, nonce: u64, network_id: &[u8]) -> [u8; POW_L1_BLAKE2B_SHA3_OUT_BYTES] {
    // BLAKE2b-512 half (keyed on DOMAIN). Length-prefixed network id => self-delimiting preimage.
    let half_b = Params::new()
        .hash_length(64)
        .key(POW_L1_BLAKE2B_SHA3_V1_DOMAIN)
        .to_state()
        .update(&(network_id.len() as u16).to_le_bytes())
        .update(network_id)
        .update(pre_pow_hash.as_byte_slice())
        .update(&nonce.to_le_bytes())
        .finalize();

    // SHA3-512 half. `sha3` has no keying, so DOMAIN is prepended explicitly; the same
    // length-prefixed (network_id, pre_pow_hash, nonce) follow.
    let mut s = Sha3_512::new();
    s.update(POW_L1_BLAKE2B_SHA3_V1_DOMAIN);
    s.update((network_id.len() as u16).to_le_bytes());
    s.update(network_id);
    s.update(pre_pow_hash.as_byte_slice());
    s.update(nonce.to_le_bytes());
    let half_s = s.finalize();

    let mut out = [0u8; POW_L1_BLAKE2B_SHA3_OUT_BYTES];
    out[..64].copy_from_slice(half_b.as_bytes());
    out[64..].copy_from_slice(&half_s);
    out
}

/// MISAKA Phase 4 (`algo_id = 4`): the 32-byte PALW-PoW seed.
///
/// ```text
/// seed = BLAKE2b-256(
///     key   = POW_L1_PALW_V1_DOMAIN,
///     input = "seed" || netid_len_le16 || network_id || pre_pow_hash64 ||
///             timestamp_le || nonce_le,
/// )
/// ```
///
/// Binds the block (`pre_pow_hash`), the network (length-prefixed id), and BOTH miner-grindable
/// header inputs (`timestamp`, `nonce`) — see the [`POW_ALGO_ID_PALW_LLM`] doc for why `timestamp`
/// must be inside the expensive computation and not merely inside the Layer-0 finalizer.
pub fn palw_pow_seed_v1(pre_pow_hash: Hash64, timestamp: u64, nonce: u64, network_id: &[u8]) -> [u8; 32] {
    let digest = Params::new()
        .hash_length(32)
        .key(POW_L1_PALW_V1_DOMAIN)
        .to_state()
        .update(b"seed")
        .update(&(network_id.len() as u16).to_le_bytes())
        .update(network_id)
        .update(pre_pow_hash.as_byte_slice())
        .update(&timestamp.to_le_bytes())
        .update(&nonce.to_le_bytes())
        .finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_bytes());
    out
}

/// MISAKA Phase 4 (`algo_id = 4`): the canonical prompt bytes fed to the PALW worker's stdin for a
/// given PoW seed. Pure printable ASCII (a fixed frame + 64 lowercase-hex chars) so the pinned
/// tokenizer's behavior is exercised only on a stable alphabet; the semantic content is irrelevant
/// — determinism of the transcript is everything. The prompt tokenizes well under
/// [`POW_L1_PALW_N_PREDICT_V1`], leaving a real greedy-decode budget (the actual work).
pub fn palw_pow_prompt_v1(seed: &[u8; 32]) -> String {
    format!("MISAKA PALW proof-of-work v1\nseed: {}\ncontinue:", faster_hex::hex_string(seed))
}

/// MISAKA Phase 4 (`algo_id = 4`): the **fixture** Layer-1 tag — the shape of a real PALW
/// projection, synthesized in-process from the seed alone. Selected (in `kaspa-pow`) by
/// `MISAKA_PALW_POW_FIXTURE=1`, mirroring the `devnet-vlt-fixture` precedent: CI and harness runs
/// exercise the whole PALW dispatch/consensus surface without the 1.2 GB pinned model. A fixture
/// node and a real-model node compute DIFFERENT tags — that is correct (they are different rule
/// sets and must not share a mesh), exactly like fixture VLT tables.
pub fn palw_fixture_l1_tag_v1(seed: &[u8; 32]) -> [u8; POW_L1_PALW_OUT_BYTES] {
    let part = |label: &[u8]| -> [u8; 64] {
        let digest = Params::new()
            .hash_length(64)
            .key(POW_L1_PALW_V1_DOMAIN)
            .to_state()
            .update(b"fixture")
            .update(label)
            .update(seed)
            .finalize();
        let mut out = [0u8; 64];
        out.copy_from_slice(digest.as_bytes());
        out
    };
    let mut tag = [0u8; POW_L1_PALW_OUT_BYTES];
    tag[..64].copy_from_slice(&part(b"output"));
    tag[64..128].copy_from_slice(&part(b"gemm"));
    tag[128..192].copy_from_slice(&part(b"schedule"));
    // Fixture "token counts": stable, obviously synthetic values (prefill = 47, decode = 81)
    // keeping the count field layout identical to a real projection.
    tag[192..196].copy_from_slice(&47u32.to_le_bytes());
    tag[196..200].copy_from_slice(&81u32.to_le_bytes());
    tag
}

/// MISAKA Phase 4b (`algo_id = 5`): assemble the 72-byte PALW-Ollama Layer-1 tag from a
/// deterministic `/api/generate` response. Shared by the HTTP runner (`kaspa-pow`) and tests so
/// the byte layout has one definition.
///
/// ```text
/// digest = BLAKE2b-512(key = POW_L1_PALW_OLLAMA_V1_DOMAIN,
///                      "output" || resp_len_le_u64 || response_bytes)
/// tag    = digest ∥ prompt_eval_count_le_u32 ∥ eval_count_le_u32
/// ```
pub fn palw_ollama_l1_tag_from_response(
    response_bytes: &[u8],
    prompt_eval_count: u32,
    eval_count: u32,
) -> [u8; POW_L1_PALW_OLLAMA_OUT_BYTES] {
    let digest = Params::new()
        .hash_length(64)
        .key(POW_L1_PALW_OLLAMA_V1_DOMAIN)
        .to_state()
        .update(b"output")
        .update(&(response_bytes.len() as u64).to_le_bytes())
        .update(response_bytes)
        .finalize();
    let mut tag = [0u8; POW_L1_PALW_OLLAMA_OUT_BYTES];
    tag[..64].copy_from_slice(digest.as_bytes());
    tag[64..68].copy_from_slice(&prompt_eval_count.to_le_bytes());
    tag[68..72].copy_from_slice(&eval_count.to_le_bytes());
    tag
}

/// MISAKA Phase 4b (`algo_id = 5`): the **fixture** tag — the 72-byte layout synthesized from the
/// seed alone, selected by the same `MISAKA_PALW_POW_FIXTURE=1` env as the worker fixture (and
/// confined to devnet by the same kaspad rail).
pub fn palw_ollama_fixture_l1_tag_v1(seed: &[u8; 32]) -> [u8; POW_L1_PALW_OLLAMA_OUT_BYTES] {
    let digest = Params::new().hash_length(64).key(POW_L1_PALW_OLLAMA_V1_DOMAIN).to_state().update(b"fixture").update(seed).finalize();
    let mut tag = [0u8; POW_L1_PALW_OLLAMA_OUT_BYTES];
    tag[..64].copy_from_slice(digest.as_bytes());
    // Stable, obviously synthetic counts in the real field layout.
    tag[64..68].copy_from_slice(&70u32.to_le_bytes());
    tag[68..72].copy_from_slice(&48u32.to_le_bytes());
    tag
}

/// Assemble the 200-byte PALW Layer-1 tag from a worker projection's replay-stable fields. Shared
/// by the real subprocess runner (`kaspa-pow`) and tests so the byte layout has one definition.
pub fn palw_l1_tag_from_projection(
    output_commitment: &Hash64,
    gemm_trace_root: &Hash64,
    operation_schedule_commitment: &Hash64,
    prefill_tokens: u32,
    decode_tokens: u32,
) -> [u8; POW_L1_PALW_OUT_BYTES] {
    let mut tag = [0u8; POW_L1_PALW_OUT_BYTES];
    tag[..64].copy_from_slice(&output_commitment.as_bytes());
    tag[64..128].copy_from_slice(&gemm_trace_root.as_bytes());
    tag[128..192].copy_from_slice(&operation_schedule_commitment.as_bytes());
    tag[192..196].copy_from_slice(&prefill_tokens.to_le_bytes());
    tag[196..200].copy_from_slice(&decode_tokens.to_le_bytes());
    tag
}

/// Difficulty-lift helper. Maps a 256-bit upstream-style target to
/// a 512-bit kaspa-pq target while preserving block-finding
/// probability under the ideal uniform-hash model:
///
/// ```text
/// Pr[X_512 ≤ target_256 << 256]
///   = (target_256 << 256) / 2^512
///   = target_256 / 2^256
///   = Pr[X_256 ≤ target_256]
/// ```
///
/// Use cases:
///
///  - Translating historical 256-bit compact-bits values into the
///    kaspa-pq comparison domain at fork activation.
///  - Sanity-checking the `from_compact_target_bits_512` decoder:
///    by construction
///    `from_compact_target_bits_512(bits) == lift_target_256_to_512(
///        Uint256::from_compact_target_bits(bits))`.
#[inline]
pub fn lift_target_256_to_512(target_256: Uint256) -> Uint512 {
    Uint512::from(target_256) << 256
}

/// `floor(2^512 / (target + 1))` returned as a [`Uint576`]. Thin
/// re-export of `Uint512::calc_work_512` so consumers can pull the
/// kaspa-pq work-computation surface from `pow_layer0` without
/// also pulling `kaspa_math` directly.
///
/// NOTE (audit L): GHOSTDAG blue-work is **intentionally** still computed by the
/// legacy `difficulty::calc_work(bits)` (32-bit-compact target), NOT this 512-bit
/// form — the kaspa-pq difficulty lift keeps the historical work unit so blue-work
/// accounting is unchanged. This helper exists for the Layer-0 512-bit PoW surface
/// and block-level derivation. Switching only *part* of the work accounting to
/// `calc_work_512` would change blue-work and split the DAG, so the two MUST NOT be
/// mixed within a single accounting domain.
#[inline]
pub fn calc_work_512(target: Uint512) -> Uint576 {
    target.calc_work_512()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::ZERO_HASH64;

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    #[test]
    fn algo_id_phase1_only_admits_kheavyhash() {
        assert!(check_algo_id_phase1(POW_ALGO_ID_KHEAVYHASH).is_ok());
        for bad in [0u8, 2, 3, 7, 0xff] {
            assert_eq!(check_algo_id_phase1(bad), Err(PowLayer0Error::UnknownAlgoId(bad)));
        }
    }

    #[test]
    fn check_algo_id_enforces_exact_network_algo() {
        // BLAKE2b-SHA3-active network: must be 3; kHeavyHash (1) and the superseded Argon2id (2) are
        // both WRONG (not just unknown) on the active path.
        assert!(check_algo_id(POW_ALGO_ID_BLAKE2B_SHA3, false, false, true).is_ok());
        assert_eq!(check_algo_id(POW_ALGO_ID_KHEAVYHASH, false, false, true), Err(PowLayer0Error::UnknownAlgoId(1)));
        assert_eq!(check_algo_id(POW_ALGO_ID_ARGON2ID, false, false, true), Err(PowLayer0Error::UnknownAlgoId(2)));
        // kHeavyHash network: must be 1, 3 is rejected.
        assert!(check_algo_id(POW_ALGO_ID_KHEAVYHASH, false, false, false).is_ok());
        assert_eq!(check_algo_id(POW_ALGO_ID_BLAKE2B_SHA3, false, false, false), Err(PowLayer0Error::UnknownAlgoId(3)));
        // PALW-worker-active network: must be 4, and PALW takes precedence over BLAKE2b-SHA3.
        assert!(check_algo_id(POW_ALGO_ID_PALW_LLM, false, true, false).is_ok());
        assert!(check_algo_id(POW_ALGO_ID_PALW_LLM, false, true, true).is_ok());
        assert_eq!(check_algo_id(POW_ALGO_ID_BLAKE2B_SHA3, false, true, true), Err(PowLayer0Error::UnknownAlgoId(3)));
        assert_eq!(check_algo_id(POW_ALGO_ID_PALW_LLM, false, false, false), Err(PowLayer0Error::UnknownAlgoId(4)));
        // PALW-Ollama-active network: must be 5, superseding every other algo.
        assert!(check_algo_id(POW_ALGO_ID_PALW_OLLAMA, true, false, false).is_ok());
        assert!(check_algo_id(POW_ALGO_ID_PALW_OLLAMA, true, true, true).is_ok());
        assert_eq!(check_algo_id(POW_ALGO_ID_PALW_LLM, true, true, true), Err(PowLayer0Error::UnknownAlgoId(4)));
        assert_eq!(check_algo_id(POW_ALGO_ID_PALW_OLLAMA, false, true, false), Err(PowLayer0Error::UnknownAlgoId(5)));
        assert_eq!(required_algo_id(false, false, true), 3);
        assert_eq!(required_algo_id(false, false, false), 1);
        assert_eq!(required_algo_id(false, true, false), 4);
        assert_eq!(required_algo_id(false, true, true), 4);
        assert_eq!(required_algo_id(true, false, false), 5);
        assert_eq!(required_algo_id(true, true, true), 5);
    }

    /// `check_algo_id_known` (pruning-proof path) accepts every algo this binary can FINALIZE —
    /// kHeavyHash (1), the superseded Argon2id (2), BLAKE2b-SHA3 (3), PALW LLM (4), PALW-Ollama
    /// (5), the committed-V2 id (6), the receipt id (7) and the heartbeat id (8) — and rejects the
    /// rest.
    #[test]
    fn check_algo_id_known_accepts_all_verifiable_algos() {
        for ok in [
            POW_ALGO_ID_KHEAVYHASH,
            POW_ALGO_ID_ARGON2ID,
            POW_ALGO_ID_BLAKE2B_SHA3,
            POW_ALGO_ID_PALW_LLM,
            POW_ALGO_ID_PALW_OLLAMA,
            POW_ALGO_ID_PALW_COMMITTED_V2,
            // 7 joined when its finalizer arm landed (ADR-0044 Unit B): this set means "this
            // binary can derive the tag", and it now can.
            POW_ALGO_ID_PALW_RECEIPT_V3,
            // 8 joined with ADR-0066 Decision 1's heartbeat lane. Its tag arm is algo-3's, shared
            // rather than copied, so "this binary can derive the tag" was true the moment the id
            // existed — and it is listed in the same commit that added the arm, which is the rule
            // the algo-6 delisting below established. Note that KNOWN is not ACCEPTED: whether a
            // network admits an algo-8 header is `Params::palw_heartbeat`'s question, one level up.
            POW_ALGO_ID_HEARTBEAT_V1,
        ] {
            assert!(check_algo_id_known(ok).is_ok(), "algo_id {ok} must be known");
        }
        for bad in [0u8, 9, 10, 0xff] {
            assert_eq!(check_algo_id_known(bad), Err(PowLayer0Error::UnknownAlgoId(bad)));
        }

        // **Audit C1, closed.** This assertion was `Err(UnknownAlgoId(6))` while the finalizer
        // had no arm for the V2 id — the mode gate reads this function to decide whether a
        // `ConsensusV2` ruleset is runnable, and claiming 6 verifiable without an arm is what let
        // a V2 network boot and then stall at block 1. The arm landed together with its carrier
        // (`StateLayer0::calculate_l1_tag`'s algo-6 arm over the header-carried
        // `PalwAttemptEnvelopeV2`), so the id is listed again, and this flip happened in that
        // same commit — exactly as the delisting comment said it must.
        assert!(
            check_algo_id_known(POW_ALGO_ID_PALW_COMMITTED_V2).is_ok(),
            "the V2 arm exists; delisting 6 now would shut a bootable ruleset"
        );

        // Knowing the V2 id is not accepting a V2 block. `required_algo_id` has no V2 arm until the
        // atomic bundle lands (ADR-0042 Decision 1), so no combination of today's fork flags demands
        // it — which is what keeps the id inert on every shipped network while its semantics exist.
        for (ollama, llm, sha3) in
            [(false, false, false), (false, false, true), (false, true, false), (false, true, true), (true, true, true)]
        {
            assert_ne!(
                required_algo_id(ollama, llm, sha3),
                POW_ALGO_ID_PALW_COMMITTED_V2,
                "no fork-flag combination may demand V2 before the bundle exists"
            );
            assert_eq!(
                check_algo_id(POW_ALGO_ID_PALW_COMMITTED_V2, ollama, llm, sha3),
                Err(PowLayer0Error::UnknownAlgoId(POW_ALGO_ID_PALW_COMMITTED_V2)),
                "a V2 header must be refused everywhere today"
            );
            // The mode seam (PR-08): with no V2 mode (`None` — every real network), the
            // mode-aware gate is the V1 gate exactly, so a V2 header is still refused…
            assert_eq!(
                check_algo_id_for_mode(POW_ALGO_ID_PALW_COMMITTED_V2, None, ollama, llm, sha3),
                Err(PowLayer0Error::UnknownAlgoId(POW_ALGO_ID_PALW_COMMITTED_V2)),
                "with no V2 mode the V2 id is refused, exactly as the V1 gate refuses it"
            );
            assert_eq!(
                required_algo_id_for_mode(None, ollama, llm, sha3),
                required_algo_id(ollama, llm, sha3),
                "no mode = the V1 cascade, byte for byte"
            );
            // …and ONLY a ConsensusV2 mode (`Some(6)`) demands and accepts it — exclusively, the
            // V1 flags no longer consulted for that network.
            assert_eq!(
                required_algo_id_for_mode(Some(POW_ALGO_ID_PALW_COMMITTED_V2), ollama, llm, sha3),
                POW_ALGO_ID_PALW_COMMITTED_V2
            );
            assert!(
                check_algo_id_for_mode(POW_ALGO_ID_PALW_COMMITTED_V2, Some(POW_ALGO_ID_PALW_COMMITTED_V2), ollama, llm, sha3).is_ok()
            );
            // A V2 network refuses the V1 ids it would otherwise have required — nothing but V2.
            assert_eq!(
                check_algo_id_for_mode(required_algo_id(ollama, llm, sha3), Some(POW_ALGO_ID_PALW_COMMITTED_V2), ollama, llm, sha3),
                Err(PowLayer0Error::UnknownAlgoId(required_algo_id(ollama, llm, sha3))),
                "a V2 network accepts the committed-V2 id and nothing else"
            );
        }
    }

    /// ADR-0044 (FP-02): the receipt-spend id (7) exists, belongs to the PALW header family, and
    /// is demanded or accepted by NOTHING — no fork-flag combination, no mode that exists today
    /// (a `ConsensusV2` network still demands 6 exclusively), and not the pruning-proof gate,
    /// whose contract is "algos this binary can verify" and whose over-acceptance is the
    /// unknown-algo remote-crash shape.
    #[test]
    fn the_receipt_v3_id_is_known_to_the_family_and_demanded_by_nothing() {
        assert!(is_palw_algo_id(POW_ALGO_ID_PALW_RECEIPT_V3), "a receipt header carries (and hashes) its spend carriage");
        // **Position, both kinds, refused together.** The level half and the work half used to be
        // two independent decisions and only one of them said no; a receipt block took level 0 and
        // still handed its descendants `calc_work(bits)` of blue work, which is fork-choice weight
        // bought with a signature. They read this one predicate now, so neither can drift.
        assert!(
            algo_id_carries_no_chain_position(POW_ALGO_ID_PALW_RECEIPT_V3),
            "a receipt header buys neither pruning-proof level nor blue work"
        );
        assert!(
            !algo_id_carries_no_chain_position(POW_ALGO_ID_PALW_COMMITTED_V2),
            "the attempt lane is where chain position comes from — its digests are inference-priced"
        );
        assert!(!algo_id_carries_no_chain_position(POW_ALGO_ID_KHEAVYHASH), "and a hash network is untouched by any of this");
        assert!(
            check_algo_id_known(POW_ALGO_ID_PALW_RECEIPT_V3).is_ok(),
            "since Unit B the pruning-proof gate can derive this tag, so it must not refuse the id"
        );
        for (ollama, llm, sha3) in
            [(false, false, false), (false, false, true), (false, true, false), (false, true, true), (true, true, true)]
        {
            assert_ne!(required_algo_id(ollama, llm, sha3), POW_ALGO_ID_PALW_RECEIPT_V3);
            assert_eq!(
                check_algo_id(POW_ALGO_ID_PALW_RECEIPT_V3, ollama, llm, sha3),
                Err(PowLayer0Error::UnknownAlgoId(POW_ALGO_ID_PALW_RECEIPT_V3))
            );
            for mode_required in [None, Some(POW_ALGO_ID_PALW_COMMITTED_V2)] {
                assert_ne!(required_algo_id_for_mode(mode_required, ollama, llm, sha3), POW_ALGO_ID_PALW_RECEIPT_V3);
                assert_eq!(
                    check_algo_id_for_mode(POW_ALGO_ID_PALW_RECEIPT_V3, mode_required, ollama, llm, sha3),
                    Err(PowLayer0Error::UnknownAlgoId(POW_ALGO_ID_PALW_RECEIPT_V3)),
                    "no mode that exists today accepts a receipt block — the two-id set arrives only with the FP bundle's wiring"
                );
            }
        }

        // The header-shape gate, since Unit B: a receipt header MUST carry a well-formed spend
        // envelope — empty is refused, junk is refused, and the V1 fence flag is not consulted
        // at all (the lane's id already means the network demanded it).
        for bound in [false, true] {
            assert!(
                matches!(
                    check_palw_commitment_shape(POW_ALGO_ID_PALW_RECEIPT_V3, &[], bound),
                    Err(PowLayer0Error::PalwCommitmentMalformed { .. })
                ),
                "bound = {bound}: an empty carriage is not a spend"
            );
            assert!(matches!(
                check_palw_commitment_shape(POW_ALGO_ID_PALW_RECEIPT_V3, &[1, 2, 3], bound),
                Err(PowLayer0Error::PalwCommitmentMalformed { .. })
            ));
            // …and the attempt lane's rule is the mirror image: its own envelope or nothing.
            assert!(matches!(
                check_palw_commitment_shape(POW_ALGO_ID_PALW_COMMITTED_V2, &[], bound),
                Err(PowLayer0Error::PalwCommitmentMalformed { .. })
            ));
        }
        let oversized = vec![0u8; PALW_COMMITMENT_MAX_BYTES + 1];
        assert_eq!(
            check_palw_commitment_shape(POW_ALGO_ID_PALW_RECEIPT_V3, &oversized, false),
            Err(PowLayer0Error::PalwCommitmentTooLong { got: oversized.len(), cap: PALW_COMMITMENT_MAX_BYTES }),
            "the cap still reports the cap it broke, before any codec runs"
        );
    }

    /// PALW-PoW seed (algo_id = 4): deterministic, and sensitive to EVERY grindable input — block,
    /// network, nonce, and (unlike the other algos' tags) the timestamp. Timestamp sensitivity is
    /// the grinding-closure property: a miner re-stamping a header must pay a fresh inference.
    #[test]
    fn palw_pow_seed_deterministic_and_binds_timestamp() {
        let net = b"kaspa-devnet";
        let a = palw_pow_seed_v1(h(0x11), 1_000, 42, net);
        assert_eq!(a, palw_pow_seed_v1(h(0x11), 1_000, 42, net), "seed must be deterministic");
        assert_ne!(a, palw_pow_seed_v1(h(0x12), 1_000, 42, net), "pre_pow_hash must change the seed");
        assert_ne!(a, palw_pow_seed_v1(h(0x11), 1_001, 42, net), "timestamp must change the seed (grinding closure)");
        assert_ne!(a, palw_pow_seed_v1(h(0x11), 1_000, 43, net), "nonce must change the seed");
        assert_ne!(a, palw_pow_seed_v1(h(0x11), 1_000, 42, b"mainnet"), "network must change the seed");
    }

    /// The worker-class calibration is pinned exactly where a single class is claimed —
    /// testnet-11 — and nowhere else; and the pinned value is a well-formed 200-byte tag whose
    /// token counts are the audited probe record's (u0: prefill 71, decode 57, both under the
    /// frozen 128 budget). Golden: a change here means the network's determinism class moved,
    /// which strands every deployed runtime — make sure that is the intended outcome (ADR-0035).
    #[test]
    fn palw_worker_calibration_scope_and_shape() {
        assert!(palw_worker_calibration_v1(b"testnet-11").is_some(), "the public PALW net must pin its class");
        assert!(palw_worker_calibration_v1(b"testnet-10").is_none(), "the hash-lane t10 pins nothing");
        assert!(palw_worker_calibration_v1(b"devnet").is_none(), "devnet deliberately pins no class");
        assert!(palw_worker_calibration_v1(b"mainnet").is_none());
        assert!(palw_worker_calibration_v1(b"simnet").is_none());
        let hex = POW_L1_PALW_WORKER_CALIBRATION_TN11_V1;
        assert_eq!(hex.len(), POW_L1_PALW_OUT_BYTES * 2, "calibration must encode a full tag");
        let mut bytes = [0u8; POW_L1_PALW_OUT_BYTES];
        faster_hex::hex_decode(hex.as_bytes(), &mut bytes).expect("calibration must be valid hex");
        assert_eq!(u32::from_le_bytes(bytes[192..196].try_into().unwrap()), 71, "prefill tokens of the audited u0 probe");
        assert_eq!(u32::from_le_bytes(bytes[196..200].try_into().unwrap()), 57, "decode tokens of the audited u0 probe");
    }

    /// The canonical PALW prompt is a pure-ASCII stable frame around the hex seed, and two seeds
    /// never render the same prompt.
    #[test]
    fn palw_pow_prompt_is_ascii_and_seed_bound() {
        let s1 = palw_pow_seed_v1(h(0x11), 1, 2, b"devnet");
        let s2 = palw_pow_seed_v1(h(0x11), 1, 3, b"devnet");
        let p1 = palw_pow_prompt_v1(&s1);
        assert!(p1.is_ascii(), "prompt must be pure ASCII for tokenizer stability");
        assert!(p1.contains(&faster_hex::hex_string(&s1)), "prompt must embed the seed hex");
        assert_ne!(p1, palw_pow_prompt_v1(&s2));
        assert_eq!(p1, palw_pow_prompt_v1(&s1), "prompt must be deterministic");
    }

    /// The fixture tag has the exact projection layout (200 bytes, counts in the tail),
    /// is deterministic, seed-sensitive, and — critically — differs from what
    /// `palw_l1_tag_from_projection` would assemble from any real worker fields (the fixture's
    /// three 64-byte parts are domain-keyed on the seed, not on model output).
    #[test]
    fn palw_fixture_tag_layout_and_determinism() {
        let seed_a = palw_pow_seed_v1(h(0x11), 1, 2, b"devnet");
        let seed_b = palw_pow_seed_v1(h(0x11), 1, 9, b"devnet");
        let a = palw_fixture_l1_tag_v1(&seed_a);
        assert_eq!(a.len(), POW_L1_PALW_OUT_BYTES);
        assert!(POW_L1_PALW_OUT_BYTES <= POW_L1_TAG_MAX_BYTES, "tag must fit the finalizer's max");
        assert_eq!(a, palw_fixture_l1_tag_v1(&seed_a), "fixture tag must be deterministic");
        assert_ne!(a, palw_fixture_l1_tag_v1(&seed_b), "fixture tag must be seed-sensitive");
        assert_eq!(&a[192..196], &47u32.to_le_bytes(), "fixture prefill count field");
        assert_eq!(&a[196..200], &81u32.to_le_bytes(), "fixture decode count field");
        // The three 64-byte parts are pairwise distinct (distinct sub-domains).
        assert_ne!(&a[..64], &a[64..128]);
        assert_ne!(&a[64..128], &a[128..192]);
    }

    /// `palw_l1_tag_from_projection` writes each replay-stable field at its documented offset.
    #[test]
    fn palw_projection_tag_layout() {
        let oc = h(0xaa);
        let gt = h(0xbb);
        let sc = h(0xcc);
        let tag = palw_l1_tag_from_projection(&oc, &gt, &sc, 29, 99);
        assert_eq!(&tag[..64], oc.as_byte_slice());
        assert_eq!(&tag[64..128], gt.as_byte_slice());
        assert_eq!(&tag[128..192], sc.as_byte_slice());
        assert_eq!(&tag[192..196], &29u32.to_le_bytes());
        assert_eq!(&tag[196..200], &99u32.to_le_bytes());
        // The finalizer accepts the 200-byte tag with algo_id = 4.
        assert!(pow_finalizer_blake2b_512(b"devnet", POW_ALGO_ID_PALW_LLM, h(0x01), 1, 2, 3, &tag).is_ok());
    }

    /// PALW-Ollama tag (algo_id = 5): documented layout, deterministic, response-sensitive, and
    /// distinct from the fixture derivation over the same bytes.
    #[test]
    fn palw_ollama_tag_layout_and_determinism() {
        let a = palw_ollama_l1_tag_from_response(b"greedy continuation", 70, 48);
        assert_eq!(a.len(), POW_L1_PALW_OLLAMA_OUT_BYTES);
        assert!(POW_L1_PALW_OLLAMA_OUT_BYTES <= POW_L1_TAG_MAX_BYTES);
        assert_eq!(a, palw_ollama_l1_tag_from_response(b"greedy continuation", 70, 48), "tag must be deterministic");
        assert_ne!(a, palw_ollama_l1_tag_from_response(b"greedy continuatioN", 70, 48), "response bytes must change the tag");
        assert_eq!(&a[64..68], &70u32.to_le_bytes());
        assert_eq!(&a[68..72], &48u32.to_le_bytes());
        // Counts are OUTSIDE the digest but inside the tag, so they still alter the finalizer input.
        let b = palw_ollama_l1_tag_from_response(b"greedy continuation", 71, 48);
        assert_eq!(&a[..64], &b[..64], "digest covers response bytes only");
        assert_ne!(a, b, "counts must alter the tag");
        // Fixture: same layout, different derivation, seed-sensitive.
        let seed_a = palw_pow_seed_v1(h(0x11), 1, 2, b"testnet-10");
        let seed_b = palw_pow_seed_v1(h(0x11), 1, 3, b"testnet-10");
        let fa = palw_ollama_fixture_l1_tag_v1(&seed_a);
        assert_eq!(fa, palw_ollama_fixture_l1_tag_v1(&seed_a));
        assert_ne!(fa, palw_ollama_fixture_l1_tag_v1(&seed_b));
        assert_ne!(&fa[..64], &palw_ollama_l1_tag_from_response(&seed_a[..], 70, 48)[..64]);
        // The finalizer accepts the 72-byte tag with algo_id = 5.
        assert!(pow_finalizer_blake2b_512(b"testnet-10", POW_ALGO_ID_PALW_OLLAMA, h(0x01), 1, 2, 3, &fa).is_ok());
    }

    /// BLAKE2b-SHA3 Layer-1 (algo_id = 3) must be DETERMINISTIC (miner and every verifier agree on
    /// the tag for a given header+nonce), 128 bytes wide, and sensitive to block, nonce and network.
    /// It must also differ from the kHeavyHash-seed and Argon2id derivations on the same inputs.
    #[test]
    fn blake2b_sha3_l1_tag_deterministic_and_sensitive() {
        let net = b"testnet-10";
        let a = blake2b_sha3_l1_tag_v1(h(0x11), 42, net);
        let b = blake2b_sha3_l1_tag_v1(h(0x11), 42, net);
        assert_eq!(a, b, "BLAKE2b-SHA3 L1 must be deterministic");
        assert_eq!(a.len(), POW_L1_BLAKE2B_SHA3_OUT_BYTES);
        assert_eq!(a.len(), 128);
        assert!(a.len() <= POW_L1_TAG_MAX_BYTES, "tag must fit the finalizer's max");
        assert_ne!(a, [0u8; POW_L1_BLAKE2B_SHA3_OUT_BYTES]);
        // The two halves are distinct hash families over the same preimage — they must not coincide.
        assert_ne!(&a[..64], &a[64..], "BLAKE2b half must differ from SHA3 half");
        assert_ne!(a, blake2b_sha3_l1_tag_v1(h(0x12), 42, net), "pre_pow_hash must change the tag");
        assert_ne!(a, blake2b_sha3_l1_tag_v1(h(0x11), 43, net), "nonce must change the tag");
        assert_ne!(a, blake2b_sha3_l1_tag_v1(h(0x11), 42, b"mainnet"), "network must change the tag");
        // Distinct from the other algos' derivations on the same input (different algo).
        assert_ne!(&a[..32], argon2id_l1_tag_v1(h(0x11), 42, net).as_slice());
        assert_ne!(&a[..32], l1_seed32_for_kheavyhash_v1(h(0x11)).as_bytes().as_slice());
    }

    /// Argon2id Layer-1 (algo_id = 2) must be DETERMINISTIC (miner and every verifier must agree on
    /// the tag for a given header+nonce) and sensitive to block, nonce and network.
    #[test]
    fn argon2id_l1_tag_deterministic_and_sensitive() {
        let net = b"testnet-10";
        let a = argon2id_l1_tag_v1(h(0x11), 42, net);
        let b = argon2id_l1_tag_v1(h(0x11), 42, net);
        assert_eq!(a, b, "Argon2id L1 must be deterministic");
        assert_eq!(a.len(), POW_L1_ARGON2ID_OUT_BYTES);
        assert_ne!(a, [0u8; 32]);
        assert_ne!(a, argon2id_l1_tag_v1(h(0x12), 42, net), "pre_pow_hash must change the tag");
        assert_ne!(a, argon2id_l1_tag_v1(h(0x11), 43, net), "nonce must change the tag");
        assert_ne!(a, argon2id_l1_tag_v1(h(0x11), 42, b"mainnet"), "network must change the tag");
        // It must differ from the kHeavyHash-seed derivation on the same inputs (different algo).
        assert_ne!(a.as_slice(), l1_seed32_for_kheavyhash_v1(h(0x11)).as_bytes().as_slice());
    }

    /// The finalizer is deterministic: same input -> same output.
    #[test]
    fn finalizer_deterministic() {
        let net = b"simnet";
        let a = pow_finalizer_blake2b_512(net, 1, h(0x11), 1_000_000, 0x1e7fffff, 42, &[7u8; 32]).unwrap();
        let b = pow_finalizer_blake2b_512(net, 1, h(0x11), 1_000_000, 0x1e7fffff, 42, &[7u8; 32]).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), POW_FINALIZER_BYTES);
    }

    /// Every input field meaningfully influences the digest. This is
    /// the self-delimiting property of the layout — varying any one
    /// field must shift the output.
    #[test]
    fn finalizer_inputs_change_digest() {
        let base = pow_finalizer_blake2b_512(b"simnet", 1, h(0x11), 100, 0x1e7fffff, 7, &[3u8; 32]).unwrap();

        let net_diff = pow_finalizer_blake2b_512(b"mainnet", 1, h(0x11), 100, 0x1e7fffff, 7, &[3u8; 32]).unwrap();
        assert_ne!(base, net_diff, "network_id must alter digest");

        // algo_id 2 is not a valid Phase 1 id, but the finalizer must
        // accept arbitrary algo_id bytes (Phase 2+ will hard-fork in
        // new ids). What matters here: changing algo_id changes the
        // digest.
        let algo_diff = pow_finalizer_blake2b_512(b"simnet", 2, h(0x11), 100, 0x1e7fffff, 7, &[3u8; 32]).unwrap();
        assert_ne!(base, algo_diff, "algo_id must alter digest");

        let pre_diff = pow_finalizer_blake2b_512(b"simnet", 1, h(0x22), 100, 0x1e7fffff, 7, &[3u8; 32]).unwrap();
        assert_ne!(base, pre_diff, "pre_pow_hash must alter digest");

        let ts_diff = pow_finalizer_blake2b_512(b"simnet", 1, h(0x11), 101, 0x1e7fffff, 7, &[3u8; 32]).unwrap();
        assert_ne!(base, ts_diff, "timestamp must alter digest");

        let bits_diff = pow_finalizer_blake2b_512(b"simnet", 1, h(0x11), 100, 0x207fffff, 7, &[3u8; 32]).unwrap();
        assert_ne!(base, bits_diff, "bits must alter digest");

        let nonce_diff = pow_finalizer_blake2b_512(b"simnet", 1, h(0x11), 100, 0x1e7fffff, 8, &[3u8; 32]).unwrap();
        assert_ne!(base, nonce_diff, "nonce must alter digest");

        let tag_diff = pow_finalizer_blake2b_512(b"simnet", 1, h(0x11), 100, 0x1e7fffff, 7, &[4u8; 32]).unwrap();
        assert_ne!(base, tag_diff, "l1_tag bytes must alter digest");

        let len_diff = pow_finalizer_blake2b_512(b"simnet", 1, h(0x11), 100, 0x1e7fffff, 7, &[3u8; 31]).unwrap();
        assert_ne!(base, len_diff, "l1_tag length must alter digest");
    }

    /// The 2-byte length prefix in front of `l1_tag` defeats the
    /// canonical-concat collision attack: two distinct (tag, netid)
    /// pairs whose concatenation is the same string must still
    /// produce different digests.
    #[test]
    fn finalizer_l1_tag_is_self_delimiting() {
        // Construction: two l1_tag values whose raw bytes differ only
        // by length-prefix boundary placement. Without the length
        // prefix this would collide; with it, the digests differ.
        let a = pow_finalizer_blake2b_512(b"net", 1, ZERO_HASH64, 0, 0, 0, b"AB").unwrap();
        let b = pow_finalizer_blake2b_512(b"net", 1, ZERO_HASH64, 0, 0, 0, b"ABCD").unwrap();
        let c = pow_finalizer_blake2b_512(b"net", 1, ZERO_HASH64, 0, 0, 0, b"").unwrap();
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
    }

    #[test]
    fn finalizer_rejects_overlong_l1_tag() {
        let too_long = vec![0u8; POW_L1_TAG_MAX_BYTES + 1];
        let r = pow_finalizer_blake2b_512(b"net", 1, ZERO_HASH64, 0, 0, 0, &too_long);
        assert_eq!(r, Err(PowLayer0Error::L1TagTooLong(POW_L1_TAG_MAX_BYTES + 1)));
    }

    /// Difficulty-lift identity at the consensus-core boundary —
    /// matches the same identity tested in `kaspa-math` but routed
    /// through this module's `lift_target_256_to_512` re-export.
    #[test]
    fn pq_difficulty_lift_identity_at_consensus_boundary() {
        for bits in [0x207fffffu32, 0x1d00ffffu32, 0x1e21bc1cu32, 486722099u32] {
            let target_256 = Uint256::from_compact_target_bits(bits);
            let via_decoder = Uint512::from_compact_target_bits_512(bits);
            let via_lift = lift_target_256_to_512(target_256);
            assert_eq!(via_decoder, via_lift, "decoder and lift disagree on bits={bits:#x}");
        }
    }

    #[test]
    fn calc_work_512_reexport_matches_math() {
        let target = Uint512::from_compact_target_bits_512(0x1e7fffff);
        let work_via_module = calc_work_512(target);
        let work_via_math = target.calc_work_512();
        assert_eq!(work_via_module, work_via_math);
    }

    /// Sanity check: the empty-input digest is non-trivial. (Catches
    /// a future accidental hard-coding to zero.)
    #[test]
    fn finalizer_empty_input_nontrivial_digest() {
        let d = pow_finalizer_blake2b_512(b"", 0, ZERO_HASH64, 0, 0, 0, b"").unwrap();
        assert_ne!(d, [0u8; POW_FINALIZER_BYTES]);
    }

    /// kaspa-pq Phase 9 (PR-9.3): the algo_id = 1 (kHeavyHash) seed
    /// derivation is deterministic, sensitive to every byte of the
    /// 64-byte pre-PoW hash, and key-separated from the other
    /// kaspa-pq BLAKE2b-256 hashers (TransactionHash, BlockHash,
    /// MuHashElementHash, …). Determinism is the basis for miner
    /// reproducibility; key-separation is the basis for not being
    /// substitutable elsewhere.
    #[test]
    fn l1_seed32_for_kheavyhash_v1_basic_properties() {
        let a = l1_seed32_for_kheavyhash_v1(h(0x11));
        let b = l1_seed32_for_kheavyhash_v1(h(0x11));
        assert_eq!(a, b, "derivation must be deterministic");

        let c = l1_seed32_for_kheavyhash_v1(h(0x12));
        assert_ne!(a, c, "different pre-PoW hashes must yield different seeds");

        // Flip the last byte of the 64-byte input; the derived seed
        // must shift.
        let mut bytes = [0x11u8; 64];
        bytes[63] = 0x12;
        let last_bit_flipped = l1_seed32_for_kheavyhash_v1(Hash64::from_bytes(bytes));
        assert_ne!(a, last_bit_flipped, "every byte of pre_pow_hash must influence the seed");

        // Key separation against the existing 32-byte BLAKE2b
        // hashers. The kHeavyHash seed must not equal any of them on
        // the same input bytes.
        use kaspa_hashes::{BlockHash, Hasher, MuHashElementHash, TransactionHash};
        let pre_pow_bytes = h(0x33).as_bytes();
        let pre_pow_slice: &[u8] = &pre_pow_bytes;
        let seed = l1_seed32_for_kheavyhash_v1(h(0x33));
        assert_ne!(seed.as_bytes(), BlockHash::hash(pre_pow_slice).as_bytes());
        assert_ne!(seed.as_bytes(), TransactionHash::hash(pre_pow_slice).as_bytes());
        assert_ne!(seed.as_bytes(), MuHashElementHash::hash(pre_pow_slice).as_bytes());
    }
}

#[cfg(test)]
mod two_lane_gate_tests {
    use super::*;
    use crate::palw_mode_v2::PalwConsensusMode;

    /// **A V2 network accepts both of its lanes, and the gate has to ask the right question.**
    ///
    /// `required_algo_id` is what a producer DECLARES; `accepts_algo_id` is what a validator
    /// ALLOWS. They differ by exactly the free-prompt receipt lane — the one that needs no model —
    /// and a gate using the first refused every block on the second.
    #[test]
    fn the_gate_accepts_both_v2_lanes_and_nothing_else() {
        let bundle = crate::palw_mode_v2::tests::conforming_bundle();
        let mode = PalwConsensusMode::ConsensusV2(bundle.clone());
        let required = mode.required_algo_id();

        // The two bonded PALW lanes, always. The heartbeat is NOT one of them since ADR-0066: it
        // has its own id (8) and its own top-level fence, and the bundle deliberately cannot see
        // that fence — `Params::palw_heartbeat_lane_open_at` is the one place that decides.
        for id in [POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_RECEIPT_V3] {
            check_algo_id_for_mode_accepting(id, required, mode.accepts_algo_id(id), false, false, false)
                .unwrap_or_else(|e| panic!("a V2 network must accept its own lane {id}: {e:?}"));
        }
        assert_eq!(
            mode.accepts_algo_id(POW_ALGO_ID_HEARTBEAT_V1),
            Some(false),
            "the bundle never admits the heartbeat id — the fence does, one level up"
        );
        assert_eq!(mode.accepts_algo_id(POW_ALGO_ID_BLAKE2B_SHA3), Some(false), "and the hash lane is not this network's");
        // The pre-V2 inference lanes and the Phase-1/2 algos are not this network's.
        for id in [POW_ALGO_ID_KHEAVYHASH, POW_ALGO_ID_ARGON2ID, POW_ALGO_ID_PALW_LLM, POW_ALGO_ID_PALW_OLLAMA] {
            assert!(
                check_algo_id_for_mode_accepting(id, required, mode.accepts_algo_id(id), true, true, true).is_err(),
                "a V2 network must not accept {id} — its lanes are exclusive"
            );
        }
    }

    /// **Every non-V2 preset is untouched, byte for byte.** `accepts_algo_id` is `None` there, and
    /// the function falls through to the comparison it always made.
    #[test]
    fn a_non_v2_network_falls_through_to_the_v1_cascade() {
        for mode in [PalwConsensusMode::Disabled, PalwConsensusMode::LegacyTn11] {
            assert_eq!(mode.accepts_algo_id(POW_ALGO_ID_PALW_RECEIPT_V3), None);
            let required = mode.required_algo_id();
            for (ollama, llm, b2s3) in [(false, false, false), (false, false, true), (false, true, false), (true, false, false)] {
                for id in 0u8..=8 {
                    assert_eq!(
                        check_algo_id_for_mode_accepting(id, required, mode.accepts_algo_id(id), ollama, llm, b2s3).is_ok(),
                        check_algo_id_for_mode(id, required, ollama, llm, b2s3).is_ok(),
                        "id {id} at ({ollama},{llm},{b2s3}) must decide exactly as it did before"
                    );
                }
            }
        }
    }
}
