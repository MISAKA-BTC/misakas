# ADR-0026: PALW v2 verification architecture — runtime-separated, class-pinned, open-kernel

Status: **Accepted (architecture).** Operating envelope unchanged: devnet, shadow mode,
consensus-visible zero-credit only. Every activation gate in the v2 design §12 stands; this ADR
moves none of them.
Date: 2026-08-15
Relates to: ADR-0007 (layered PoW), ADR-0021 (superseded PALW PoW record — kept as history),
ADR-0024 (verified-LLM token-weighted BFT),
[`palw-full-logits-trace-v2-design.md`](../palw-full-logits-trace-v2-design.md) (operative
safety model), the detailed / VPS-canonical-worker / secure-OTA design set,
`consensus/core/src/palw_v2.rs` (frozen preimages).

## Context

### Where the v2 scheme stands

The PALW v2 execution scheme (informally "algo 2"; canonically `PALW_EXECUTION_ALGO_ID_V2`, a
PALW-internal namespace value that is **not** the header `pow_algo_id = 2`) commits one pinned-LLM
execution to a `full_logits_sequence_root`: each decode step's full logits vector
(n_vocab = 248 320, f32) becomes a `job_context_hash`-bound event hash, the ordered events a
domain-separated Merkle root, and that root a keyed outer hash binding job, network, runtime,
class and token budget. Implemented so far, all consensus-inert: the worker's `v2-job` path
(token-ID envelopes, exact decode, fail-closed non-finite handling, build-measured
`RuntimeManifestV2`), golden-vector boot gating, `misaka-palw-agent` Phase A (admission →
supervised execution → response re-binding; QUARANTINED rather than wrong), kaspad
`--compute-endpoint` (health probing, capability handle), and `PalwCapabilityDeclarationV2`
(v2 design §16 — typed and signed-message-frozen, not yet chain-accepted).

The honesty baseline (v2 design §4) is load-bearing for every decision below: a trace root is an
**audited commitment, not a cryptographic proof**. Any claimant can announce an arbitrary root;
only canonical replay by independent bonded verifiers, bonds/slashing, challenge windows and
reward maturity make a false one costly. Acceptance security is economic, not cryptographic.

### The external reference: Ambient's Proof of Logits (public-surface survey, 2026-08-15)

Ambient is the only other production-intent system we know of that binds L1 economics to verified
LLM inference ("Proof of Logits"). A survey of the `ambient-xyz` GitHub organization (11 public
repositories, surveyed 2026-08-15) found:

| public repo | role | distance from PoL |
|---|---|---|
| `vllm` | official vLLM fork; `main` ≈ upstream, no PoL-specific code visible | runtime |
| `llama.cpp-ambient-bin` | compiled binaries only, per its description "Binaries compiled with Ambient verification support." — a four-backend matrix (`macos-arm64`, `ubuntu-cuda-x64`, `ubuntu-rocm-7.2-x64`, `ubuntu-vulkan-x64`), date-versioned `llama-2026.07.21.17-…` | runtime, verification-enabled, **patch source unreleased** |
| `auction-api` / `-client` / `-interface` / `-listener` | Solana-side job submission, auction-winner wait, `wait_for_job_verification` | consumes verification *outcomes*; computes none |
| `tokenizer` | standalone tokenization | pinned component outside the runtime |
| `ambient-miner-benchmark`, `async-threadpool` | miner calibration, plumbing | periphery |

Not public anywhere on that surface: the commitment/preimage format, the logits hash spec,
challenge selection, validator recomputation code, the comparison/tolerance rule, slashing logic,
stake accounting, and the consensus integration — i.e. every parameter of their own
`P_detect × S > G` inequality. `auction-listener`'s lockfile shows no dependency on any
verification crate, and no repo uses submodules; the listener expects a plain multi-repo
checkout. The most consistent reading — a reading, not a fact (see "What the survey could not
establish") — is a **separated architecture**: inference runtimes (vLLM *and* llama.cpp) surface
logits; an unreleased verification component turns them into commitments; the chain layer only
waits on verification status. Upstream vLLM already exposes raw-logits modes, which is exactly
the interface a runtime-external verifier needs.

Two things follow for us. First, the separated shape we already built now has independent
convergent precedent — across two runtimes and four hardware backends. Second, the parts Ambient
withholds are precisely the parts our acceptance model cannot afford to withhold.

## Decision

**1. The three-layer separation is the frozen architecture: scheme kernel / execution / node.**
(a) The *scheme kernel* — every domain key (`misaka-palw/<name>/v2`), preimage layout and
canonical encoding — lives in `consensus/core/src/palw_v2.rs`, frozen by golden-vector tests,
with no runtime dependency. (b) *Execution* sits behind the agent boundary: a runtime adapter
(today the patched-llama.cpp `misaka-palw-worker`) speaks `PalwJobEnvelopeV2` framed Borsh under
the half-close wire contract, and `misaka-palw-agent` gates boot on golden vectors, supervises
per-job, and re-binds every response. (c) The *node* consumes capability, re-verifying everything
it is told (`--compute-endpoint`; declaration gate §16.3) and declaring only pinned identity
hashes. Ambient patches its runtimes and ships the result; where its commitment logic lives is
unobservable from outside. Ours is deliberately **not** in the runtime: with N runtimes, a
commitment defined inside each fork drifts into N dialects — the v2 domain-key incident (two
documents, two prefixes, an honest-refutes-honest permanent fork caught only at reconciliation)
is the in-house proof. The kernel is single-sourced in consensus-core; runtimes are replaceable
adapters beneath it.

**2. A hardware backend is a determinism class, and the comparison rule is full-length exact.**
Ambient's four-backend binary matrix leaves open whether its verifiers compare across backends
under a tolerance or verify per-backend; neither is published. We resolve the question for
MISAKA from our own measurements and choose **per-class exactness**:

* Same job, same model: Metal root `ba3b9994…` ≠ CPU root `d04672dc…` (2026-08-13 smoke) — the
  class split is real and visible at the root.
* F16 profile: all-x86 vendors agreed 8/8 seeds; arm-vs-x86 agreed 7/8, the single miss an
  argmax flip on a prefill batch-GEMM **near-tie** — so arm sits outside the x86 class.
* The registry Q8_0 artifact split even EPYC vs Broadwell (4/8) — class membership is a property
  of the exact artifact, not of "x86".

Divergence concentrates at near-ties, which is exactly where an ε-band is undecidable; and any
published tolerance is a standing allowance for *cheaper approximate execution* (lower-precision
kernels, pruned paths) that stays in-band with high probability — it un-prices the work the
scheme exists to price. Tolerance would also break the event model: event hashes commit exact
bytes, so Merkle single-event openings (the future TraceVM challenge) verify only under
exactness; an ε-rule would require shipping raw ~1 MB logits vectors per challenged event.
Therefore: within a class, full 64-byte equality; across classes, **no comparison** — a replay
refutes only within the declared `runtime_class_id`, and a cross-class mismatch refutes nothing
(same output with a different trace is the expected physics, and is itself the class-violation
signal when a receipt claims otherwise). A new backend (CUDA, ROCm, Vulkan, Metal, a vLLM-based
GPU adapter) enters as a **new class** with its own canonical artifact, golden set, and its own
v2-design §12 gate. GPU class granularity (per driver? per GPU generation?) is unknown until
measured and is not assumed — CUDA is not presumed batch-invariant. Class membership is
manifest-hash exact, never a label: the golden-set gate that compared a class *label* let a
different build pass (`f9ab6ab`), and a determinism class is a claim about **pairs** of hosts —
nothing with fewer than two measured sides is a class.

**3. Distribution is a per-class canonical artifact bundle; the source stays public.** Adopt the
operational pattern Ambient demonstrates at four-backend scale — date-versioned, per-platform,
verification-enabled runtime artifacts — as our release unit: one bundle per class containing
the bit-identical static worker binary, its `RuntimeManifestV2`, the full-length golden vector
set, the GGUF/tokenizer pins and the launch verifier, signed under the secure-OTA role model
(distribution and activation stay separate). "Build it yourself per host" is rejected on our own
evidence: per-host builds silently minted divergent artifacts (the ggml-OpenMP delta,
`2825d99`), and a fleet that compiles is a fleet whose class membership is unmeasured.
Reproducible-build checking from public source (`patchset_root`, full worker/patch source, the
manifest's build-measured hashes) remains a public *audit* path — but bit-identity of the
distributed artifact is the *membership* rule. This resolves the manifest's `"unpinned"` fields:
a class registration MUST reject any manifest still carrying them. Unlike Ambient, no part of
the runtime patch is binary-only: the bundle is a convenience and a pin, never the only form of
the code.

**4. The verification kernel is open — the deliberate inverse of Ambient's split.** Everything
Ambient's public surface withholds, MISAKA publishes and freezes: preimage layouts (golden-frozen
in `palw_v2.rs`), the comparison rule (full-length exact, §2 above), challenge selection and
committee sampling, bond, slash, maturity and challenge-window parameters, and the acceptance
inequality itself (v2 design §10) with **measured** replay costs on the pinned fleet. This is
not a transparency preference; it is structural. (a) Acceptance security here is economic:
`P_detect × S > G` cannot be audited by anyone who cannot see challenge selection, the
comparison rule, or S. A private tolerance/challenge spec makes the security claim unfalsifiable
— and our own algo-5 history shows what unfalsified looks like: the output-text commitment
seemed sound until measurement found a 1-distinct constant, forgeable without model execution.
Publication plus measurement killed it in one cycle; a private spec would have preserved it.
(b) The committee model *requires* that any bonded operator can stand up a verifier from public
material alone. If the verification component is vendor-supplied and closed — the position
Ambient's surface implies for its own network — then the verifier set is permissioned by the
vendor, "independent committee" degenerates into "the vendor's deployment", and slashing on the
word of a closed component is ungovernable. (c) There is nothing load-bearing to hide: the
domain keys are public keyed-BLAKE2b separations, not MAC secrets (v2 design §4.1); obscurity
buys this scheme no security, so its only effect would be to gate verifiers.

**5. The chain consumes verification outcomes; it never computes them — and there is no auction
tier.** The Ambient chain layer's shape (a listener that *waits* on verification status) matches
ours: the only consensus-visible PALW objects are declarations (`PalwCapabilityDeclarationV2`),
future certificates/challenges, bonds and slashes; no header-validation path ever runs
inference. The block-validity condition stays `valid permanent hash PoW AND (PALW certificate
absent OR valid under its activation stage)` — the algo-4 lesson (one inference per header made
IBD hours-per-day and forced the per-header replay tax onto every verifier) is why v2 is an
overlay credit above a hash floor, not a header algorithm. Where we diverge from Ambient by
design: PALW jobs are **self-originated** — the seed is chain-derived (v2 design §9), there is
no orderer, no job market and no job fee in this scheme's scope, so the entire auction stack has
no MISAKA equivalent and none is planned here.

**6. Tokenization lives outside the runtime.** The worker takes token IDs only — it never
tokenizes, normalizes or applies templates — and `tokenizer_id`/`tokenizer_sha256` pin the
profile in the manifest and job context. Ambient's standalone `tokenizer` repo is convergent
precedent that the tokenizer is a separately-pinned component: a tokenizer drift is an identity
change, not a runtime patch.

## What the survey could not establish

The Ambient facts above are the *public* surface as of 2026-08-15; the architecture reading is
inference. Specifically undetermined: whether PoL code sits on a non-default branch of the vLLM
fork (A), in private repositories (B), or exists only as the compiled verification-enabled
binaries (C) — B/C fit the evidence best; whether their verifiers compare across backends, and
under what rule; and everything listed as non-public in Context. No binary was reverse-engineered
for this ADR. A strings/symbol/CLI-surface audit of the published `llama.cpp-ambient-bin`
tarballs could narrow runtime-embedded vs external-daemon and is noted as **optional competitive
intelligence — never a design input**: every decision above must stand on our own measurements
and requirements even if every guess about Ambient is wrong, and this ADR is written so that it
does.

## Consequences

* **A GPU/vLLM path is class work, not scheme work.** A new runtime adapter must provide: the
  per-decode full-logits event surface into the unchanged kernel preimages; FP-environment
  probes before and after model load; a build-measured manifest with zero `"unpinned"` fields;
  golden generation/self-test; and the envelope identity gate. The agent, wire contract, and
  `palw_v2.rs` do not change. Class granularity on GPUs is a measurement campaign (§12-style,
  per class) before any registration.
* **Release engineering becomes consensus-adjacent.** The bundle *is* the class: cutting one is
  a pinning act under OTA roles, and a bundle hash mismatch is a membership rejection, not a
  warning. Date-versioned bundle names (the `llama-2026.07.21.17` pattern) are adopted.
* **Any tolerance proposal is a new scheme version.** There is no parameter for ε; relaxing
  exactness changes the preimage meaning and therefore requires a new `trace_scheme_id`, new
  golden sets, a new ADR and re-activation from zero-credit.
* **Publication duty before credit.** The §10 inequality ships with numbers — challenge
  probability, committee size/confirmations, slash amounts, maturity, measured per-class replay
  cost and capacity at p99 — as a public document, before the first non-zero credit. An
  unpublished parameter is an unmet §12 gate.
* ADR-0021 remains the historical record of the header-PoW era (algo 4/5) and now points here;
  the v2 design document remains the operative safety model and gate list. This ADR adds no new
  activation and changes no consensus behavior.
