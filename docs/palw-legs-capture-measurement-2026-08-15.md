# PALW execution-commitment legs — worker capture, and the measurement it stands on

**Date:** 2026-08-15 · **Branch:** `palw-llm-pow-iso` · **Stage:** Land (consensus-inert)
**Code:** `misaka-palw-worker/src/shim.c`, `misaka-palw-worker/src/main.rs`,
`consensus/core/src/palw_legs.rs` (producer half) · **Normative:** ADR-0026 §2, ADR-0027 §1–§2

The schema (`misaka-palw/execution-commitment/v1`) was frozen first, deliberately, so the capture
code would have exactly one layout to hit. This is that capture, plus the measurement that decides
whether it is usable at all.

## 1. What the runtime now captures

| Leg | Source | Committed as |
| --- | --- | --- |
| Logits | unchanged v2 full-logits trace | `full_logits_trace_root` (frozen, untouched) |
| Activation | graph node `l_out-<il>` — the post-block residual stream, after the control-vector hook | exact f32-LE rows, one leaf per (call, tap, position) |
| Checkpoint | `llama_state_seq_get_data(seq 0)` at every interval boundary | `state_root` over those bytes, leaves chained from a job-bound genesis |
| GEMM/tile | **absent by design** — blocked on step-function pinning; adding it is a new scheme version | — |

The residual stream is the tap choice that matters: every later block, and therefore the logits,
is downstream of it, so a wrong row cannot be hidden by anything the model does afterwards.

The two identities the schema left opaque are now answered, by declaration, and both name the
llama.cpp commit — they are claims about one runtime's graph and one serializer's format, not
about "an LLM":

```
tap_semantics  = llama.cpp@030ebb558a5820b444a8f836ed5cdd46c9b4bd7a/graph-node/l_out-{il}/post-block-residual-stream/f32-le/row-per-position/v1
                 → tap_semantics_id  f1ae8024b0648174c3da9ccd34105b47…
state_layout   = llama.cpp@030ebb558a5820b444a8f836ed5cdd46c9b4bd7a/llama_state_seq_get_data/seq-0/v1
                 → state_layout_id   3c35d740666c1b78c649a0d423545abd…
tap layers     = [6, 12, 18, 23]   (quartiles + last block of 24)
interval       = 8 decode calls    (= the worst-case replay length a challenge costs)
scheme id      = 426f7278957cf5c552e5a58399373571…
```

## 2. The gate: is capture logits-neutral?

llama.cpp accepts a capture callback **only** at context creation, and installing one changes how
`ggml_backend_sched` computes: instead of one whole-split compute, it computes sub-ranges cut at
every tensor the callback asks for. Whether that changes the *arithmetic* is a property of each
backend's fusion and scheduling — and it decides the whole increment, because the legs bind the
**frozen v2 logits root**. If capture moves that root, a capturing executor and a non-capturing
verifier disagree about an honest execution, and the composite commitment is worthless.

So `--mode v2-legs-selftest` replays the registered golden set with capture ON and demands the
same roots the goldens hold. Measured on this host (M-series, 2026-08-15):

| Class | `runtime_class_id` | Golden jobs | Logits roots unmoved |
| --- | --- | --- | --- |
| `apple-metal-arm64` | `03a3c66c221fa263…` | 4 | **4 / 4** |
| `cpu-only aarch64` | `18bc9d20bfb17183…` | 4 | **4 / 4** |

Capture is logits-neutral on both classes on this host. That is a *measurement*, not a property:
it must be re-run per backend and per llama.cpp bump, and a future backend that fails it is not a
bug to paper over — it means capture is a distinct determinism class there, and the honest
outcomes are to say so and register it as one. The selftest exits non-zero on any drift.

## 3. What else was measured

* **Leaf counts are the canonical ones.** `taps × (P + D−1)`: 4/108/444/36 for the four golden
  jobs; checkpoints `⌊(D−1)/8⌋`: 0/1/1/0 — including two jobs that exercise the empty-leg
  sentinel rather than a root over nothing.
* **Reproducible across runs.** Two full runs produced byte-identical execution-commitment roots,
  which is the check that matters for the checkpoint leg: it says `llama_state_seq_get_data` is
  deterministic here, i.e. the KV serialization carries no uninitialised padding or run-varying
  state. A checkpoint leg that failed this would be unopenable by an honest replay.
* **The taps read real tensors.** 592 rows captured across the four jobs, **0 of them all-zero**.
  A backend whose tensor read silently no-ops would produce a perfectly stable, perfectly
  reproducible commitment to nothing — the one capture failure that does not announce itself — so
  the worker counts zero rows and aborts a job whose capture is entirely zero.
* **The composite root is class-scoped, exactly like the logits root.** Same job, same prompt:
  every execution-commitment root differs between the Metal and CPU classes. That is correct and
  is the same rule as before — cross-class comparison refutes an honest receipt.

## 4. Producer discipline

The worker does not construct commitments. It streams captured material into
`PalwLegsCommitmentBuilderV1` in `palw_legs.rs`, which is the adjudicator's own module, so every
structural fault in `PalwLegsFaultV1` is unreachable by construction:

* coordinates are checked against `canonical_activation_leaf_index` and must arrive in tree order
  (tap-major within the prefill call) — a capture loop that walks the execution the wrong way is a
  build error, not a wrong root a verifier finds later;
* rows must be exactly the profile's hidden dim, and a non-finite value aborts the **build** (the
  fail-closed rule: an honest execution emits no receipt rather than a refutable one);
* a short leg fails at `finish()` — the partial-capture commitment is exactly what
  `ActivationLeafCountNotCanonical` convicts, and an executor must never be the one to emit it;
* checkpoints chain from the job-bound genesis with the interval doing the counting.

A tap that captured nothing, a tap that fired twice in one call (a split ubatch), a non-F32 or
non-2-D node, or a model whose depth/width is not the pins' — each aborts the job with no output.

## 5. Interfaces

* `--mode v2-legs-job` — same envelope in; a `PalwLegsJobResultV1` frame out, carrying the v2
  result **unchanged** (same struct, same bytes, same goldens) plus the binding. Two objects
  rather than a widened `PalwJobResultV2`, because whether a class produces legs is a registration
  fact. `validate_coherence()` refuses a result whose two halves describe different executions.
* `--mode v2-legs-selftest` — the gate above, plus a JSON report of the identities and roots.
* `--mode v2-job` — unchanged, and verified unchanged: the baseline selftest still reproduces
  4/4 goldens on both classes. A context opened without taps installs no callback and therefore
  runs the byte-identical scheduler path the goldens were measured on.

```bash
export MISAKA_PALW_GGUF=/path/to/Qwen3.5-2B-Q4_K_M.gguf
export MISAKA_PALW_GOLDEN=misaka-palw-worker/golden/qwen35-2b-v2.metal-arm64.golden
cargo build --release -p misaka-palw-worker
./target/release/palw-worker --mode v2-legs-selftest
```

For the CPU class, build with `MISAKA_PALW_CPU=1 MISAKA_LLAMA_SRC=<cpu-only llama tree>` and use
the `qwen35-2b-v2.cpu-aarch64.golden` set.

## 6. Open, and deliberately not done here

* **Nothing consumes this.** No consensus validation, fork choice, header pipeline or acceptance
  path reads a legs commitment; the node agent still drives `v2-job`. Wiring the driver is the
  next increment.
* **Registration.** `tap_semantics_id`, `state_layout_id`, the tap layers and the interval are
  declared by the worker and measured here; they become network facts only at class registration.
* **The GEMM/tile leg** stays absent until the step function is pinned at tile granularity.
* **Challenge cost.** The interval sets the replay length, but the per-class p99 cold-replay cost
  is still unmeasured — an ADR-0026 §12 gate item, not something this increment closes.
* **Cross-host, same-class.** Neutrality was measured on one host per class. The class claim is
  about *pairs* of hosts, and that measurement has not been re-run for the capturing path.
