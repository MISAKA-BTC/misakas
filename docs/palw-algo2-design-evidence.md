# algo_id = 2 (PALW LLM, logits-bound) — the measurements the design rests on

Written while the fleet builds, so the implementation that follows cites evidence rather than
intent. Every row is something I ran, not something I expect.

## Why the previous design died (measured, 2026-08-12)

| measurement | result |
|---|---|
| Ollama greedy continuation, 10 uniform-random seeds, shipped N=16 | **1 distinct** — a constant |
| The pinned `POW_L1_PALW_OLLAMA_CALIBRATION_V1`, first 64 bytes | **equals** the BLAKE2b-512 of that constant string (checkable with no runtime) |
| Seed-derived 24-word prompt, 60 seeds, N=16 | 40 distinct, **p_max 0.117 → ~3.1 bits** min-entropy, top-5 cover 38% |
| Same, N=32 (double the budget) | **~3.3 bits** — the budget is not the lever |
| Ollama `/v1/completions` `logprobs` on 0.32.8 | **absent/null** — no way to bind logits through this API |

Conclusion, and the reason a prompt fix was rejected: a tag that can only commit to OUTPUT TEXT
is bounded by the entropy of that text, and a small model at temperature 0 collapses to a handful
of attractors no matter what it is fed. ~3 bits is a dictionary attack, not a proof of work.

## Why the worker's tag is structurally different (read + measured)

`misaka-palw-worker` commits `gemm_trace_root = keyed-BLAKE2b(trace_events)` where each event is a
digest of **the full logits vector after every decode call** (`n_vocab = 248_320` f32 values —
`TRACE_SCHEME = "full-logits-per-decode-call/keyed-blake2b-512/v1"`). The forgery that killed
algo 5 required guessing a 16-token string drawn from ~3 bits of effective entropy; forging this
requires producing ~250k floats per decode call that the model itself computes. Argmax collapsing
does not imply logits collapsing — the logits are a continuous function of the prompt.

Measured on the pinned 2B GGUF (Metal build, three seeds, decode fixed at 16 — the exact regime
where Ollama's text was constant):

```
seed 00112233 prefill=68 decode=16  out=c306e02f779de0a6d70c  trace=ecb8dee6a03008d5db14
seed ffeeddcc prefill=69 decode=16  out=8208ff8e5f9522c2872e  trace=d4ecd6f507932afca384
seed a1b2c3d4 prefill=81 decode=16  out=6011beed44d86b060682  trace=3695392f91729bdaebb2
```

Both commitments move with the seed. Note the worker's OUTPUT is already seed-dependent at 16
tokens where Ollama's was constant — the runtimes tokenize/prime differently — so prompt design
is now defense in depth rather than the load-bearing part.

## The one gate that is still open

The CPU-profile note from 2026-08-11 recorded byte-identical traces across rerun, `verify` mode
and five concurrent replicas **on one machine**, and named the remaining box explicitly: *a second
machine reproducing a first machine's `gemm_trace_root`*. That is exactly what algo 2 needs, and
it is a harder property than argmax agreement: agreeing on a full-logits digest means agreeing on
every low bit of ~250k floats per call.

It also gates more than the PoW — `select_verifiers` draws a VLT committee only from validators
sharing a determinism class, so a Linux fleet that cannot reproduce a peer's trace cannot run the
overlay either.

### RESULT (2026-08-12): the gate PASSES, and it was tested the honest way

h1 (AMD EPYC 6c) and h2 (Intel Broadwell 8c, f16c masked), both on llama.cpp `030ebb558` built
`-DGGML_NATIVE=OFF -DGGML_METAL=OFF -DGGML_BLAS=OFF -DGGML_ACCELERATE=OFF -DGGML_OPENMP=OFF`,
worker via `MISAKA_PALW_CPU=1`, same GGUF (`aaf42c8b…`, verified on both):

```
prompt              h1-EPYC trace             h2-Broadwell trace
1 "alpha bravo…"    b2497591355c561429156d08  b2497591355c561429156d08
2 "zulu yankee…"    dcb625b9dff54024f6527667  dcb625b9dff54024f6527667
3 "日本語の…"        8784d0582791926735a5f9df  8784d0582791926735a5f9df
```

**Cross-machine: identical on 3/3. Input-sensitive: 3 distinct traces for 3 prompts.**

Both halves matter, and measuring only the first is the mistake that produced the algo-5 disaster:
agreement on a CONSTANT is not determinism. This run asserts the trace moves with the input AND
agrees across vendors in the same experiment, so neither property can be mistaken for the other.

Consequences:
* A logits-bound PoW (algo 2) is viable on a heterogeneous x86 fleet.
* `select_verifiers` can draw a real committee on that fleet — the VLT overlay is unblocked too,
  which was the *other* thing this box was holding.
* The class is (arch, build profile incl. no-openmp, GGUF). Not the CPU vendor: EPYC and Broadwell
  are one class, measured.

### Discovered while getting there (both now fixed)

* `build.rs` linked Apple's `-lc++` unconditionally — the "portable CPU profile a Linux fleet can
  audit within" had never been built on Linux (`4be9cad`).
* ggml-cpu links OpenMP by default on Linux and not under Apple clang, so every earlier "CPU
  profile" measurement was of a structurally different build than the fleet would have run. Under
  OpenMP the matmul work split and reduction order come from an external scheduler, not from
  ggml's threadpool at the pinned thread count — the profile's core claim. `no-openmp` is now part
  of `CPU_BUILD_PROFILE` (`2825d99`).

## Implementation constraint discovered while testing

The worker's `--n-predict` is a **total** budget (prefill + decode), not a decode budget: a
68-token prompt with `--n-predict 16` aborts. The consensus constant must therefore be expressed
as prefill-aware, or the prompt kept short enough that a fixed total is always sufficient. Ollama's
`num_predict` was decode-only, so this is a real difference between the two runtimes and exactly
the kind of thing that silently produces a different tag if assumed rather than checked.
