# MISAKA Studio

A local LLM desktop app: search Hugging Face, download a model, load it, chat with it — and serve
an OpenAI-compatible API on `localhost` while you do. Windows, macOS and Linux. Nothing you type
leaves the machine.

What it has that other local-LLM apps do not: **every completion is recorded with the identity of
what produced it** — the model's `h_M`, the runtime's `h_R` and determinism class, and commitments
to the prompt and the answer, derived exactly the way MISAKA consensus derives them.

```
  Studio UI  (React — or any OpenAI client)
      │  HTTP:  /v1/…  and  /api/v1/…
  MISAKA Runtime API              misaka-studiod
      │  InferenceBackend
  LLM backend                     llama.cpp · MLX · (MISAKA's own runtime, later)
      │
  GPU / CPU
```

The UI and the runtime are separate processes that talk over that HTTP boundary and nothing else.
The desktop shell is a window around the same bundle the runtime serves. Which means: the API other
applications use is the API this app uses, and it cannot rot without the app breaking first.

## Status

**v0.1 — working MVP.** Model management, the runtime, chat, the performance monitor and the API
are implemented and tested. The llama.cpp backend has been run end to end against a real
`llama-server` — load, streaming, token counts, runtime identity — and the desktop shell opens,
spawns its runtime and takes it down again. The Network tab joins the MISAKA network: it has
**mined real PALW blocks** — a Studio-supervised bonded producer on a locally minted
`ConsensusV2` chain, with a second bonded node verifying and filing receipts (see
*Joining the MISAKA network*). Not yet done: MLX is wired but has never run on a Mac, no CUDA or
Metal machine has executed a model here, and the public-testnet join is unexercised from the
build environment (no P2P egress).

## Quick start

```bash
# 1. The runtime and the UI bundle
cd misaka-studio
cargo build --release                    # misaka-studiod
npm --prefix ui install && npm --prefix ui run build

# 2. An engine. The Studio drives llama.cpp; it does not bundle it.
#    Install llama.cpp so `llama-server` is on PATH, or point the setting at your own build.

# 3. Run
./target/release/misaka-studiod --ui-dir ui/dist
#   → http://127.0.0.1:1338
```

No engine installed and just want to see the app? `--backend mock` streams canned replies with no
model at all:

```bash
./target/release/misaka-studiod --ui-dir ui/dist --backend mock
```

### The desktop app

```bash
cd misaka-studio/desktop/src-tauri
cargo tauri dev          # or: cargo tauri build   (needs the Tauri CLI: cargo install tauri-cli)
```

The shell spawns `misaka-studiod` itself, waits for it to be healthy, and stops it on exit — unless
one was **already** running on the configured port, in which case it attaches to that one rather
than starting a rival that would load the same model into the same GPU twice.

Linux needs the webview development packages (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`); macOS and
Windows have their webviews already. Bundle icons for Windows and macOS are generated from
`desktop/src-tauri/icons/icon.png` with `cargo tauri icon`.

## Using it as an API

Point anything that speaks OpenAI at `http://127.0.0.1:1338/v1`:

```bash
curl http://127.0.0.1:1338/v1/chat/completions \
  -H 'content-type: application/json' \
  -d '{"model":"Qwen3-4B-Instruct-Q4_K_M","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

`/v1/chat/completions`, `/v1/completions` and `/v1/models`, with SSE streaming and the same chunk
shape OpenAI sends. Naming a model that is not loaded loads it. `top_k`, `min_p` and
`repeat_penalty` are accepted as extra fields, because those are what local engines actually
expose.

The Studio's own API lives under `/api/v1` — models with fit verdicts, catalog search, downloads,
metrics, settings, provenance records. `misaka-studiod --help` lists the flags;
`misaka-studiod --check` prints where everything resolved to.

**The default bind is `127.0.0.1` and there is no authentication.** Binding anywhere else without
`--api-key` is refused at startup rather than served: an open inference endpoint on a shared
network is the kind of mistake that stays open for a week before anyone notices.

## What it does that is worth knowing about

**It tells you whether a model will run *before* you download it.** A size in bytes does not
answer that question. Every model is shown with its real memory bill — weights plus the KV cache at
the context it would actually load with, plus compute overhead — against what this machine has,
including Apple Silicon's unified memory (where "0 MB VRAM" is the most misleading thing an app can
say).

**Downloads resume and are verified.** Hugging Face publishes each LFS object's SHA-256; a finished
file is checked against *that*, not against itself. A partial download lives in a `.part` file and
never appears as a model until it is one.

**The engine is a child process, not a library.** A model that crashes llama.cpp on a driver fault
takes the engine with it, not the app — and the engine can be updated without rebuilding the
Studio.

**GPU offload is planned, not guessed.** "Auto" computes how many layers fit after the KV cache and
scratch buffers are accounted for, and says so in the UI: `23/33 on GPU`.

## Joining the MISAKA network

The Network tab is the participation ladder, and every rung is the same node with more at stake —
on this network there is no separate miner program: **the thing that runs the model is the thing
that makes the block.**

* **Mining classes, as a list you can act on.** A block on MISAKA is won by verified LLM
  inference in one of the chain-registered classes — `PALW-BASE-0` (the deterministic floor,
  600‰, needs nothing), `PALW-QWEN25-A16` (200‰, converted locally from public Qwen2.5 weights),
  `QWEN36` (200‰, a 34 GiB Qwen3.6-35B artifact, downloadable and digest-pinned). Each card
  shows its share, its artifact requirement, this machine's readiness — including an honest
  "this machine cannot run this class" when the artifact exceeds RAM — and, when a node is
  running, the class's **live on-chain status** from the node's own `--palw-dump-classes` table.
  The artifact download reuses the model download pipeline, verified against the chain-pinned
  SHA-256.
* **Observer** — point the tab at any reachable node RPC and read the chain.
* **Verifier** — run a full node; on this chain syncing *is* verifying, no bond required. With a
  bonded key the same node takes panel duty and files receipts on producers' claims.
* **Producer (miner)** — a bonded ML-DSA-87 key, a bond outpoint, a pay address. The Studio
  launches and supervises the node (`--palw-produce --palw-panel …`), streams its
  `[palw-producer]`/`[palw-panel]` lines into the activity feed, and — because a person putting
  a bonded key on the line must be able to audit what ran — always displays the **exact command
  line**, reproducible without the Studio. First run without a bond registers one
  (`--palw-register-bond`); the printed outpoint goes into settings and then mines.

The node is driven over its JSON workflow-RPC (`--rpclisten-json`, loopback only), supervised as
a child process the same way engines are, and stopped with SIGTERM first so RocksDB closes clean.
`/api/v1/network` carries all of it, so any client — not just this UI — can see the ladder.

This has been demonstrated end to end: a Studio-configured producer mined hundreds of accepted
PALW-BASE-0 blocks on a locally minted testnet-11-preset chain (the same genesis ceremony that
launched the public network), while an independent bonded seat filed 270 `Valid` receipts on its
claims. What that run did and did not prove — and how to reproduce it — is recorded in
[`docs/misaka-studio-palw-demo.md`](../docs/misaka-studio-palw-demo.md) in the misakas
repository.

## Provenance — the part that is MISAKA's

The long path is `Inference → Deterministic Execution → Inference Hash → Verification → Compute
Credit → PALW → MISAKA Network`. This version implements the first and third links and nothing
beyond them. There is no chain client here, no bond, no credit.

That is the design decision, not a shortcut. Verification can be added on top of a record that
exists; a record cannot be reconstructed after the fact. An app that does not write down which
artifact answered a prompt has destroyed the evidence.

So each completion produces an [`InferenceRecord`](crates/misaka-studio-core/src/provenance.rs):

| field | what it is |
|---|---|
| `h_M` | model identity — keyed BLAKE2b-512 over the GGUF's SHA-256, size, filename, repo and revision |
| `h_R` | runtime identity — engine commit, patch digest, build number, build profile |
| `class_id` | determinism class — the set of runtimes expected to agree bit for bit |
| `prompt_commitment` / `output_commitment` | commitments to the bytes, under Studio-local domains |
| `inference_hash` | what the whole record reduces to |
| `replayability` | `deterministic`, `seeded_sampling` or `unrepeatable` — stated, never implied |

The first three are **the consensus derivations** — same keyed BLAKE2b-512, same domain keys, same
field order as `consensus/core/src/vlt.rs`, cross-checked in tests against vectors produced by a
third implementation. A model downloaded here already carries the `h_M` a validator would compute
for it.

Timing is recorded and deliberately **not** committed: a verifier re-running the job on other
hardware must reach the same `inference_hash`.

Records are JSONL at `<data dir>/inference-records.jsonl` — readable with `tail -f` and `jq`,
auditable without this app. Prompt and completion **text** is not stored unless you turn on
`provenance.keep_transcripts`: the record commits to the bytes with a hash, and a provenance log
that quietly duplicates every conversation is a second copy of your data in a place you did not
choose.

## Layout

```
misaka-studio/
  crates/misaka-studio-core/      GGUF reader, quantization table, hardware probe, fit
                                  arithmetic, settings, provenance derivations, the PALW
                                  mining-class catalog
  crates/misaka-studio-runtime/   backends, model store, Hugging Face catalog, downloads,
                                  metrics, record log, node supervisor + wRPC client,
                                  the HTTP API, `misaka-studiod`
  ui/                             React + Vite + Tailwind; one bundle, served or loaded from disk
  desktop/src-tauri/              the Tauri v2 shell
  NOTICE                          attribution — Jan (Apache-2.0), llama.cpp, MLX
```

It is a **separate cargo workspace**, excluded from the repository root's. A desktop app's
dependency graph has no business in the lockfile a validator builds from.

## Adding a backend

Implement [`InferenceBackend`](crates/misaka-studio-runtime/src/backend/mod.rs) — `load`, `unload`,
`generate`, `descriptor`, `availability` — and add an arm to `build_backend`. Nothing above that
trait names an engine. If the engine already speaks OpenAI over HTTP, `ChildEngine` supplies the
process supervision, health wait and SSE parsing, and the backend is about eighty lines (see
`llamacpp.rs`).

`descriptor()` is the part to get right: it becomes `h_R`, and a backend that cannot prove its
build flags must record the literal `unknown` rather than a plausible string. An identity derived
from a guess is worse than none.

## Tests

```bash
cd misaka-studio && cargo test           # runtime and core
npm --prefix ui run build                # typecheck + bundle
```

The mock backend exists so the streaming, metrics and provenance paths are testable with no GPU and
no multi-gigabyte download — which is what makes them testable in CI at all.

### Against a real engine

The mock cannot tell you whether the llama.cpp backend actually starts a process, survives the
health wait, parses that engine's SSE framing, or reads a version banner a real binary printed. So
there is a test that does, and a fixture small enough to make it cheap:

```bash
# A ~50 kB llama-architecture GGUF with random weights. It generates nonsense; nothing here tests
# what a model says.
python3 testing/make_tiny_gguf.py /tmp/misaka-models/tiny-llama-F32.gguf

MISAKA_TEST_LLAMA_SERVER=/path/to/llama-server \
MISAKA_TEST_MODELS_DIR=/tmp/misaka-models \
  cargo test -p misaka-studio-runtime --test llamacpp_e2e -- --nocapture
```

It skips itself, loudly, when those are not set. Three bugs were found by running it and by no
other means: llama.cpp's current version banner (`version: 0.3.0-dev (build 1, commit 90c26fc)`)
parsed to a commit that was a whole sentence; `-fa` as a bare flag is now an error, so every load
died in a usage message; and a model with no chat template answered `400 unordered_map::at`.

## Licensing

MISAKA Studio is ISC, like the rest of this repository. Jan (Apache-2.0) is the reference
implementation this was designed against; llama.cpp (MIT) and MLX (MIT) are driven as external
processes and not bundled. No closed-source application was used, copied or reverse-engineered.
See [NOTICE](NOTICE) for the details and for what must happen if Jan source is ever vendored here.
