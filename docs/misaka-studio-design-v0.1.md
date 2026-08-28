# MISAKA Studio — design v0.1

**Status:** implemented and running (2026-08-28). This describes what is in `misaka-studio/`, the
decisions behind it, and — as precisely as the rest — what it deliberately does not do yet.

## 1. What it is for

A person with a laptop should be able to run an open-weight model without a terminal: find one,
see whether it fits, download it, load it, talk to it. That is the whole product, and Jan and
LM Studio have both shown it is achievable.

MISAKA Studio has a second purpose the others do not: **it is the point at which inference becomes
evidence.** The network this repository implements settles blocks by re-deriving work that a
producer claims to have done, and every stage of that — deterministic execution, an inference
hash, a panel verdict, compute credit — starts from knowing exactly which artifact ran under
exactly which runtime. An app that answers a prompt and forgets what answered it has destroyed
that information at the only moment it existed.

So the Studio records it from the first version, and implements nothing else of the chain.

## 2. Architecture

```
  Studio UI ──────────── React bundle, served by the runtime or loaded by the shell
      │                  (also: any OpenAI-compatible client)
      │ HTTP  /v1/…  /api/v1/…
  MISAKA Runtime API ─── misaka-studiod: model store, catalog, downloads, metrics, records
      │ InferenceBackend (Rust trait)
  LLM backend ────────── llama.cpp (child process) · MLX (child process) · mock
      │
  GPU / CPU
```

Four layers, three process boundaries, and each boundary was chosen for a reason:

**UI ↔ runtime is HTTP, and it is the public API.** The window has no private channel. The chat
box calls `/v1/chat/completions` exactly as a third-party client does. This costs a little — the
UI cannot cheat by reaching into runtime state — and buys the thing that matters: the endpoint
other applications depend on cannot rot without the app visibly breaking.

**Runtime ↔ engine is a process, not a library.** A model that trips a GPU driver bug takes down
llama.cpp; if llama.cpp were linked in, it would take down the app, mid-conversation, with no
message. As a child it has its own address space, its own OOM kill and its own segfault, and the
runtime survives to say what happened and show the engine's last lines of stderr. It also means a
user can update their engine — for a new architecture, a new quantization — without waiting for a
Studio release.

The cost is paid in identity: a binary we did not build cannot tell us its CMake flags, so
`h_R` records the literal `unknown` for what cannot be proven. See §4.

**The shell is a window, not the app.** `misaka-studio` (Tauri v2) spawns `misaka-studiod`, waits
for health, and opens a webview pointed at the bundle. If a runtime is already listening on the
configured port and identifies itself as ours, the shell attaches instead of spawning — so opening
the app while a headless runtime is serving does not start a rival that loads the same model into
the same GPU twice.

## 3. Components

| Crate / directory | What lives there |
|---|---|
| `crates/misaka-studio-core/src/gguf.rs` | Header-only GGUF reader. Bounded everywhere: every length in that format is attacker-controlled. |
| `…/quant.rs` | `LLAMA_FTYPE` → scheme, effective bits per weight, quality tier. Header first, filename second. |
| `…/hardware.rs` | CPU, RAM, and accelerators via `nvidia-smi` / `rocm-smi` / `sysctl`. Unified memory is a first-class case, not a zero-VRAM GPU. |
| `…/model.rs` | The memory bill (weights + KV cache at a given context + overhead) and the fit verdict. |
| `…/provenance.rs` | The consensus derivations and the inference record. §4. |
| `…/settings.rs` | Schema, per-platform paths, atomic save. |
| `crates/misaka-studio-runtime/src/backend/` | `InferenceBackend`, the shared child-process engine driver, llama.cpp, MLX, mock. |
| `…/store.rs` | Model scan, sidecars, cached digests, deletion. |
| `…/catalog.rs`, `…/download.rs` | Hugging Face search and file listing; resumable, verified downloads. |
| `…/metrics.rs`, `…/records.rs` | Hardware + throughput sampling; the JSONL inference log. |
| `…/api/` | `/v1` (OpenAI) and `/api/v1` (Studio), SSE, static UI, auth. |
| `ui/` | React 19 + Vite + Tailwind 4. One bundle, three ways of being loaded. |
| `desktop/src-tauri/` | The shell. |

It is a **separate cargo workspace**, named in the root `Cargo.toml`'s `exclude`. A desktop app's
dependency graph — an HTTP server, a TLS stack, a webview binding — has no business in the
lockfile a validator builds from.

## 4. The provenance seam

The long path is:

```
Inference → Deterministic Execution → Inference Hash → Verification → Compute Credit → PALW → MISAKA Network
```

v0.1 implements **the first and third links**, and no more. There is no chain client, no bond, no
credit, and no attempt at bit-reproducible execution across machines.

That split is the design decision, and it runs the other way from the usual instinct to stub the
interesting parts. Verification is *additive*: it can be built later on top of records that
already exist. A record is not additive: nothing later can reconstruct which artifact answered a
prompt in March.

### What is recorded

```rust
InferenceRecord {
    model:              Option<ModelIdentity>,   // h_M + the fields it was derived from
    runtime:            RuntimeIdentity,         // h_R, class_id, descriptor
    params:             SamplingCommitment,      // temperature, top_p, top_k, min_p, penalty, max_tokens, seed
    prompt_commitment:  Digest64,
    output_commitment:  Digest64,
    prompt_tokens, completion_tokens: u64,
    inference_hash:     Digest64,                // the commitment the whole record reduces to
    replayability:      Deterministic | SeededSampling | Unrepeatable,
    // measured, and NOT committed:
    started_at_unix_ms, duration_ms, time_to_first_token_ms, tokens_per_second,
}
```

### Why the derivations are the consensus ones

`derive_model_weights_hash`, `derive_runtime_hash` and `derive_runtime_class_id` are byte-for-byte
`kaspa_consensus_core::vlt`'s: same keyed BLAKE2b-512, same domain keys (`misaka-vlt-model-identity-v1`
and friends), same field order. A model the Studio downloads therefore already carries the `h_M` a
validator would compute for it, and a runtime it drives carries the `h_R` and class id a validator
would register.

They are **duplicated rather than imported**. Depending on `kaspa-consensus-core` would drag
RocksDB, the P2P stack and ML-DSA into a desktop build on three platforms, for five hash
functions. What keeps the copy honest is that the test vectors were produced by a *third*
implementation — Python's `hashlib.blake2b` — so agreement is a cross-check rather than a copy
agreeing with itself. If either side changes, `provenance::tests` fails.

### What the prompt commitment covers

The **conversation as the runtime received it**, in a length-prefixed canonical encoding — not the
token sequence the engine ran. Two things follow.

First, the encoding has to be injective, and the obvious one is not: `role: content` joined by
newlines lets a single user message reading `a\nassistant:b` flatten to the same bytes as the
two-message exchange `[user "a", assistant "b"]`, so one conversation could be committed and
another claimed. Every field is therefore length-prefixed (`canonical_prompt_bytes`), and a raw
`/v1/completions` prompt is tagged apart from a chat conversation carrying the same text.

Second, the step from conversation to tokens — the chat template and the tokenizer — lives inside
the GGUF, and `h_M` binds the GGUF. So `(h_M, prompt_commitment)` determines the token sequence
without this layer re-implementing a template it would only get subtly wrong.

### What is deliberately outside the commitment

Wall-clock time, tokens per second, the machine's name. All recorded, none committed: a verifier
re-running the same job on different hardware must reach the same `inference_hash`, and a digest
that included timing would fail every honest replay.

### Honesty about replayability

`Replayability` is computed, not assumed. Greedy decoding (`temperature == 0`) is
`Deterministic`; sampling with a recorded seed is `SeededSampling`; sampling with a
runtime-chosen seed is `Unrepeatable`, and the record says so. Seed 0 and "no seed" produce
different commitments — conflating them would let an unrepeatable run pass as a seeded one.

Likewise `h_R`: an engine that will not identify itself records `unknown` for its commit and patch
digest rather than a plausible string, and the UI says the build flags cannot be proven. An
identity derived from a guess is worse than none — it is a number that will not match the machine
it claims to describe, discovered only when something starts comparing them.

### Determinism classes

`class_tag` for an external engine is `misaka-studio/{backend}/{os}-{arch}/{accelerator}/v1`. That
scoping is the honest one: ggml ships separate hand-written kernels per architecture, and a NEON
reduction and an AVX2 reduction sum a vector in different orders. Two runtimes in one class are
expected to agree bit for bit; two in different classes are not, and must never be paired against
each other.

### What connects this to the network later

Nothing in the Studio needs to change for verification to arrive. A record already carries every
input a verifier needs; `provenance::InferenceRecord::verify` re-derives the hash from the record
and the bytes. The missing pieces are external: a publisher, a panel, and the chain-side registry
of `(class_id, artifact_root)` that `consensus/core/src/palw_registry.rs` already has.

## 5. Decisions worth writing down

**Context is part of the memory bill.** A 20 GB model on a 24 GB card fits — until 128 k of
context asks for another 16 GB of KV cache. Sizing from the file alone is why models that "should
fit" die on the first long conversation, so every estimate is `weights + kv(ctx) + overhead`, and
the default context is the largest power of two that actually fits this machine.

**GPU offload is planned, not guessed.** "Auto" solves for the layer count that fits after the KV
cache and scratch buffers, and the UI shows the result (`23/33 on GPU`). Offloading one layer too
many is an out-of-memory error at load time, which users read as "the model is broken".

**Downloads are verified against the repository, not against themselves.** Hugging Face's `lfs.oid`
is the file's SHA-256, published before the download starts. A mismatch discards the file rather
than leaving a corrupt model to fail later inside llama.cpp with a message about tensors.

**Partial downloads are not models.** Bytes land in `<name>.gguf.part`; the scanner only sees
`.gguf`. Cancelling keeps the part file, and the next attempt resumes it with a `Range` request.

**A sidecar, not a database.** `<model>.gguf.misaka.json` holds the repository, revision and
digest — the facts a GGUF cannot carry and `h_M` needs. Beside the model, so moving 200 GB of
models to another disk moves their provenance with them.

**Loopback by default, and a refusal rather than a warning.** The API has no authentication out of
the box because the default bind is `127.0.0.1`. Binding anywhere else without `--api-key` is
refused at startup: an open inference endpoint on a shared network is a mistake that stays open
for a week.

**Transcripts are opt-in.** Records commit to the prompt and completion with hashes. The text is
stored only if the user turns it on, because a provenance log that quietly duplicates every
conversation is a second copy of their data in a place they did not choose.

**A mock backend, and it is never a fallback.** It makes the streaming, metrics and provenance
paths testable with no GPU and no multi-gigabyte download — which is what makes them testable in
CI at all. Selecting it is explicit, every reply says what it is, and its class tag
(`misaka-studio-mock/v1`) is distinct, so a mock record can never be mistaken for a real one.

**Flash attention is a tri-state that defaults to silence.** Not a bool. `-fa` as a bare flag was
accepted by llama.cpp for years and is now an error (`expected value for argument`);
`--flash-attn on` is accepted now and was not then. Current engines default to `auto` and decide
per backend, better than this app can from outside — so `Auto` passes no flag at all, which is the
only setting compatible with every engine version. Found by running one: the bare flag turned
every load into a usage message.

**A model without a chat template gets one named explicitly.** Current llama.cpp already falls
back to ChatML, so this is not a workaround — it is about *which* template, staying fixed. The
provenance argument that `(h_M, prompt_commitment)` determines the tokens holds because the
template is in the GGUF; for a model without one the renderer is the engine's built-in default, a
value that can change between engine versions and silently re-render the same conversation.

**Stopping means stopping.** The Stop button aborts the HTTP request; the runtime drops the
response; the engine stops generating. A "stop" that only hid the output would leave the GPU busy
for another thousand tokens.

**The runtime cannot outlive its window.** The shell's exit handler stops it — but a force-quit
never runs that handler. So the runtime is spawned with a pipe on stdin and exits on EOF: the
kernel closes the pipe when the shell dies, however it dies. (Polling the parent's PID was tried
first and does not work — a force-quit leaves a zombie with the right PID and start time, and the
runtime kept serving. Measured, not reasoned.)

## 5a. What has actually been run

Claims in a design document are cheap. These were measured on this machine:

* **llama.cpp end to end** — a real `llama-server` (built from source, commit `90c26fc`) loading a
  real GGUF, streaming a completion, reporting its own token counts, and being identified by its
  version banner. 255 ms to load, 217 tokens/s, first token in 24 ms on a 4-core CPU with the
  fixture model. `crates/misaka-studio-runtime/tests/llamacpp_e2e.rs` reproduces it; it skips
  itself unless pointed at an engine and a model directory.
* **The fixture** — `testing/make_tiny_gguf.py` writes a ~50 kB llama-architecture GGUF with
  random weights, so that test needs a build but not a download.
* **The desktop shell** — opened under Xvfb, spawned its runtime, and took it down again when
  force-killed.
* **The UI** — all four views in a real browser against the running runtime, light and dark, with
  no console errors.

Three of the bugs above were found this way and by no other means: the version banner in its
current shape, the bare `-fa` flag, and a `400 unordered_map::at` from a model with no chat
template.

## 6. Platforms

| Platform | Engine | Status |
|---|---|---|
| macOS, Apple Silicon | llama.cpp (Metal), or MLX | Unified memory is modelled properly; MLX is implemented and untested on hardware |
| Windows / Linux, NVIDIA | llama.cpp (CUDA) | `nvidia-smi` supplies device memory and utilisation |
| Windows / Linux, AMD | llama.cpp (ROCm) | `rocm-smi` parsed by header name |
| Anywhere | llama.cpp (CPU) | Always available; the fit verdict says what it will cost |

The Studio does not bundle an engine. A packaged build may ship `llama-server` beside the
executable (the resolver looks there first); otherwise the user's own build is used, which is what
someone who compiled with specific flags wants.

## 7. Adding a backend

Implement `InferenceBackend` — `load`, `unload`, `generate`, `descriptor`, `availability` — and add
an arm to `state::build_backend`. Nothing above that trait names an engine. If the engine speaks
OpenAI over HTTP, `ChildEngine` already supplies process supervision, the health wait, the SSE
parser and the log ring buffer, and the backend is about eighty lines.

`BackendKind::Misaka` is reserved for the deterministic runtime this repository already carries for
PALW (`misaka-palw-base0`, `misaka-palw-reference2`). That is the eventual point of the seam: a
class whose execution is adjudicable, driven from the same UI, recorded in the same shape.

## 8. Licensing

The Studio is ISC, like the rest of this repository. Jan (Apache-2.0, Menlo Research) is the
reference implementation it was designed against; the debt is documented in `misaka-studio/NOTICE`
along with what must happen if Jan source is ever vendored (`third_party/jan/`, licence and NOTICE
intact, change notices per Apache-2.0 §4). llama.cpp and MLX are MIT and are driven as external
processes, not bundled. No closed-source application was used, copied, or reverse-engineered.

## 9. What is not done

* **MLX is untested on hardware.** The code path exists and reports itself unavailable off Apple
  Silicon; nobody has run it on a Mac yet. llama.cpp, by contrast, is verified end to end (§5a).
* **No CUDA or Metal machine has run this.** The detection, the offload planning and the class
  tags are written and unit-tested; what has actually executed a model here is a CPU build.
* **`BackendKind::Misaka` refuses to run**, which is the intended state until the in-tree runtime
  is wired: it reports unavailable with a remedy and errors on load, rather than handing the work
  to llama.cpp under a record that names MISAKA. It is listed in `/api/v1/runtime/backends` so it
  is discoverable, and left out of the Settings dropdown so it cannot be selected into a dead end.
* **Conversations live in the window's local storage**, not the runtime. Moving them behind the API
  would make history available to other clients and survive a browser profile reset.
* **No embeddings endpoint** (`/v1/embeddings`), no tool calling, no MCP.
* **One model at a time.** The backend holds a single engine; serving two models concurrently needs
  a pool, and the memory arithmetic to decide whether that is even possible.
* **Nothing publishes records.** By design, for this version.
