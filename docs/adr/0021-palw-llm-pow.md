# ADR-0021: PALW LLM proof-of-work (`algo_id = 4`/`5`), at one block per 120 s

Status: **SUPERSEDED FOR REWARD/POW ACTIVATION** — historical implementation record only.
The Ollama path was disabled by `9736aec` after its output-text commitment was empirically shown
forgeable without model execution. The worker full-logits path is now experimental and limited to
devnet, shadow mode and consensus-visible zero-credit observation. See
[`palw-full-logits-trace-v2-design.md`](../palw-full-logits-trace-v2-design.md) for the current safety model,
namespace rules and activation gates, and ADR-0026 for the successor (v2 / "algo 2") verification
architecture. Mainnet remains inert.
Date: 2026-08-11
Relates to: ADR-0007 (layered PoW), ADR-0008 (64-byte pre-PoW hash), ADR-0024 (verified-LLM
token-weighted BFT), `docs/PALW` Open-then-Audit paper (rev 1.3), `misaka-palw-worker`

## Context

The chain already trusts deterministic pinned-LLM computation for consensus weight: the VLT
overlay's voting power is real Qwen3.5-2B inference, replay-verified byte-for-byte by committees
(78/78 replays reproduced `R_j` on the live 5-node devnet). The PoW itself, however, was still a
hash race (`algo_id = 3` BLAKE2b∥SHA3 on t10, kHeavyHash on devnet): the electricity buys
nothing but lottery tickets.

The Open-then-Audit paper's accounting motivated binding a worker to **one execution** and using
full canonical replay when the interval count is small (`q ≤ 18` ⇒ verify everything). Later
review established an important limit: a trace root is an audited commitment, not a cryptographic
proof that computation occurred. A claimant can publish an arbitrary root; making it survive an
honest replay requires reproducing the canonical logits or evading/corrupting the audit process.
Any credit therefore depends on committee independence, bond/slashing, challenge windows and
reward maturity. Full replay also costs approximately one primary execution and is not described
as cheap verification in the current design.

## Decision

**Layer-1 tag = one deterministic inference.** `POW_ALGO_ID_PALW_LLM = 4` slots into the ADR-0007
layered PoW unchanged: the Layer-0 BLAKE2b-512 finalizer still binds
`(network, algo_id, pre_pow_hash, timestamp, bits, nonce, l1_tag)` and compares against the
512-bit lifted target. The tag is the replay-stable projection of one `misaka-palw-worker` run —

```
seed    = BLAKE2b-256(key=misaka-l1-palw-llm-v1,
                      "seed" ∥ netid_len ∥ network_id ∥ pre_pow_hash64 ∥ timestamp ∥ nonce)
prompt  = "MISAKA PALW proof-of-work v1\nseed: <hex64>\ncontinue:"
run     = pinned Qwen3.5-2B, greedy argmax, n_predict = 128 (frozen consensus constant)
l1_tag  = output_commitment ∥ gemm_trace_root ∥ operation_schedule_commitment
          ∥ prefill_tokens ∥ decode_tokens                  (200 bytes)
```

`gemm_trace_root` chains a digest of the full logits vector of every decode call. This legacy name
does not imply that every GEMM intermediate is committed; the current design calls the value
`full_logits_sequence_root`. An unaudited claimant can announce an arbitrary value. A verifier
re-runs the worker (`verify` **is** `self-job` recomputed), and only an exact canonical replay or a
failure of the verification process can make that claim survive. This ADR's direct-PoW activation
is superseded; the commitment remains available only for staged zero-credit evaluation.

**Grinding closure — why the seed binds `timestamp`.** For cheap tags the finalizer's own
`timestamp`/`nonce` binding suffices: re-hashing after a timestamp tweak costs the same as a new
attempt. At a ~10⁹× tag-to-finalizer cost ratio, any header input adjustable *without*
re-inference becomes a free BLAKE2b grinding dimension (millisecond timestamps × ±132 s tolerance
≈ 2·10⁵ free attempts per inference) that collapses the PoW back to hashing. The two
miner-grindable inputs zeroed out of the pre-PoW hash are exactly `nonce` and `timestamp`; the
seed therefore binds both. `bits` is DAA-fixed, everything else is inside `pre_pow_hash`.

**Difficulty, work, and levels are untouched.** The finalizer output is uniform, so compact-bits
targets, the DAA, blue-work accounting (still the legacy 256-bit `calc_work`), and block levels
(top-256 projection) all work unmodified. One attempt simply costs ~1–3 s of Metal inference
instead of nanoseconds, so the equilibrium difficulty is ~p≈1/attempts-per-10 s.

**0.1 bps on devnet.** One block per 10 seconds, so one inference is a meaningful fraction of the
block interval. (The PUBLIC testnet's interval is 120 s — see "Block interval" below; devnet keeps
the faster rate because its fixture tag costs nothing.) `Bps<const BPS>` cannot express sub-integer rates; `BlockrateParams::new_deci_bps()`
spells out the same formulas at λ = 0.1 (k = 4, every-block window sampling, merge 360 /
finality 4 320 / pruning 10 800 blocks, maturity 10 — wall-clock durations identical to the
10-bps net). The devnet difficulty window is 264 blocks ≈ the same 2 641 s duration the sampled
661-slot window models at ≥1 bps. Integer `bps()` truncates to 0 at this rate, so the coinbase
emission schedule was generalized from `per_second.div_ceil(bps)` to
`(per_second × ttpb).div_ceil(1000)` — bit-identical on every integer-bps network, exact ×10
here (~370.47 KAS per 10 s block = the same 37.047 KAS/s rate). Genesis bits drop to
`0x207fffff` (p ≈ 1/2 per inference; the old `0x1e21bc1c` is 2⁻⁴³ per attempt — unreachable),
which re-genesises devnet.

**Fail loud, not wrong.** PALW worker errors are never header-dependent (the prompt is a fixed
frame far below the ceiling), so a missing/broken worker means the node cannot judge *any*
header. `calc_block_level_check_pow_layer0` panics on `PalwUnavailable`/`PalwWorkerFailed`
instead of returning `false` — silently rejecting every valid block would stall the node and ban
honest peers. This is the same stance as the VLT devnet fence (a kaspad missing its runtime
panics at the first template).

**Fixture mode.** `MISAKA_PALW_POW_FIXTURE=1` derives the 200-byte tag in-process from the seed
(the `devnet-vlt-fixture` precedent): CI and harnesses exercise the whole dispatch surface with
no 1.2 GB model. A fixture node and a real-model node compute different tags — different rule
sets that must not share a mesh, by design.

**Resource control.** Worker spawns are serialized process-wide (each loads the 1.2 GB model;
the pruning-proof path validates headers in parallel and would otherwise be a memory cliff), the
pipe-drain-before-wait pattern from the VLT runtime is applied (llama.cpp's model-load stderr
alone overflows the 64 KiB pipe buffer), and completed tags are cached by seed so the header
pipeline, block-level derivation and proof validation pay one inference per attempt.

**Mining.** `misaminer` branches on the template's `algo_id`: PALW mines sequentially (one
worker inference per nonce, clock-derived start so rigs don't duplicate attempts, 20 s template
refresh to stay ahead of the moving past-median) — the rayon all-nonce scan would fork-bomb
worker subprocesses.

**Block interval (decided 2026-08-12, T = 120 s).** Validator load is `(M-1)/M · r / T` for
per-header replay `r`; measured r ≈ 12-26 s for the 2B/F16 profile at N = 16 decode tokens, and
estimated 30-60 s for the Qwen3.6-35B-A3B runtime this network intends to adopt by algo fork.
Because the model forks on-chain and the block rate does not, T is chosen to fit BOTH: ~15 % load
today, 17-33 % after the 35B fork. `BlockrateParams::new_seconds_per_block` derives every
dependent depth from the same duration constants `Bps<BPS>` uses, so the 12 h finality / 1 h merge
/ 100 s maturity wall clocks are unchanged; `TESTNET_DNS_PARAMS` re-sizes every VLT window the
same way (14-day evidence stays 14 days). Emission is rate-preserving by construction — the
per-block subsidy is `(per-second value × ttpb).div_ceil(1000)`.

## Consequences

* Devnet is re-genesised (bits) and its consensus fingerprint moves; all four presets' pinned
  fingerprints moved because `pow_palw_activation` entered the params hash. Simnet/mainnet stay
  `never()`.
* **testnet-10 is re-genesised too** (the "-bs3" precedent, now "-palw"): trivial genesis bits,
  the 120 s blockrate, `crescendo_activation = always` (the legacy 88_657_000 emission fork score
  belonged to the superseded chain), and a wall-clock-preserving ÷100 re-sizing of every
  block/DAA/blue-score-denominated `TESTNET_DNS_PARAMS` window (inherited unchanged, the
  "14-day" unbond would have become ~3.8 years). A mid-chain BPS change was rejected: unlike
  the algo-id cut-offs, the blockrate has no forked-params machinery, and building it
  (crescendo-style) is far heavier than a testnet re-genesis.
* kaspad refuses to start on a PALW network without the worker runtime (actionable startup
  error instead of a first-header panic), and refuses `MISAKA_PALW_POW_FIXTURE=1` outside
  devnet — a mis-exported fixture var must not mint a private fork of the public testnet.
* **Historical addendum (2026-08-11; superseded by `9736aec`): testnet-10 ran the OLLAMA flavor,
  `algo_id = 5`.**
  The public fleet is Ubuntu VPSes that cannot run the Metal-pinned worker; the runtime is a
  host-local Ollama serving the pinned Qwen model (`MISAKA_PALW_OLLAMA_MODEL`). Same seed,
  prompt and grinding closure as algo 4; the tag commits to the greedy response bytes + token
  counts because the API exposes no per-decode logits — at the time this was assumed to remain
  model-work-priced,
  and the determinism class was described as (Ollama version, model digest, architecture), enforced
  operationally by `scripts/misaka-palw-ollama-setup.sh`'s calibration line. Measurement later
  showed that the response collapsed to a low-entropy constant and was forgeable without model
  execution; this path MUST remain disabled and MUST NOT be used as a determinism calibration.
* IBD replays one inference per header (~1–3 s): a full day of chain is ~8 640 blocks ≈ hours of
  inference. Acceptable for devnet; a public rollout needs the paper's sampled-audit tier or
  trusted-checkpoint IBD before the chain is long.
* VLT `DnsParams` windows are block counts, so their wall-clock stretches 100× (epoch 100 blocks
  ≈ 17 min). Consensus invariants (U ≥ R+E) are unchanged; harness timings must budget for it.
* Determinism is pinned to the Metal runtime class (the VLT precondition stands: ≥4
  Apple-Silicon machines, or build the Linux/CPU deterministic profile before widening the
  hardware set).
* One nonce = one subprocess = one model load (~1–2 s) even before inference. A resident-worker
  protocol (keep the model hot across attempts) is the obvious miner optimization; it changes no
  consensus rule and can ship later.
