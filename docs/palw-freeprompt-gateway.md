# The free-prompt gateway — run your own LLM, mine with the same inference (ADR-0044)

Status: FP-06/07 landed and measured on the pinned model. Consensus-inert: nothing here submits
to a chain yet (the executor rail is FP-08); what it produces today is the user's answer and the
outbox artifact every later step consumes.

## What this is

```
your app ──POST /v1/chat/completions──▶ misaka-palw-gateway ──▶ palw-worker --mode v3-job
                                              │                        │
                                   OpenAI-style reply          ONE inference:
                                   + roots + CU in-band        answer + trace/output/schedule roots
                                              │
                                    outbox artifact (framed result + JSON summary)
```

One inference. The same run that answers you is the run whose commitment can later certify and
mine (ADR-0044). There is no second, mining-only lane anywhere in this path.

## Build

```bash
MISAKA_LLAMA_SRC=$HOME/Downloads/misaka-palw-runtime/llama.cpp cargo build --release -p misaka-palw-worker
cargo build --release -p misaka-palw-gateway
```

The worker needs the pinned tree (its `build.rs` refuses to build blind) and, at run time, the
pinned GGUF via `MISAKA_PALW_GGUF`. The gateway needs only the worker.

## Configure

`identity.json` — who is accountable for this gateway's work (the bond's executor identity, as
registered on chain; hex is 64-byte-value hex, i.e. 128 chars):

```json
{
  "network_domain": "…128 hex…",
  "class_id": "…128 hex…",
  "bond_txid": "…128 hex…",
  "bond_index": 0,
  "executor_pubkey": "…hex…",
  "operator_id": "…128 hex…"
}
```

`anchor.json` — the freshness binding; refresh it from your node (the gateway re-reads it per
request, so an external refresher loop is enough):

```json
{ "anchor_block": "…128 hex…", "anchor_daa": 123456 }
```

## Run

```bash
MISAKA_PALW_GGUF=$HOME/Downloads/misaka-palw-runtime/models/Qwen3.5-2B-Q4_K_M.gguf \
./target/release/misaka-palw-gateway \
  --worker ./target/release/palw-worker \
  --outbox ~/.misaka-palw-outbox \
  --identity identity.json \
  --anchor anchor.json \
  --quantum-cu 1000
```

Then point any OpenAI-compatible client at it:

```bash
curl -s http://127.0.0.1:8790/v1/chat/completions \
  -H 'content-type: application/json' \
  -d '{"messages":[{"role":"user","content":"What is 2+2? Answer in one short sentence."}],"max_tokens":24}'
```

The reply is a normal chat completion plus a `misaka` object: `fp_job_id`, the three commitment
roots, the derived CU, and the artifact path.

## What the artifact is, and is not

`<outbox>/fp-job-<id>.result.borsh` is the framed `PalwFpWorkerResultV3`; `<id>.json` is the
human summary. Together they hold everything the executor rail needs to assemble the on-chain
commitment — and the summary names, honestly, what is still pending before submission:

1. trace chunk retention + the DA manifest (the worker returns roots only at v1);
2. the ML-DSA-87 signature over the claim id (the signer sidecar's job, never this process's);
3. commitment transaction assembly and submission (the executor rail, FP-08).

## Boundaries to know

- **Prompts are public.** PublicDA is the only weight-bearing mode: the committed job carries
  the token ids whole. Do not point private material at a gateway whose outbox feeds a chain.
- **Prompt budget**: single-batch prefill caps the prompt at 512 tokens; prompt + ceiling must
  fit the context window. Long-context profiles are a future class identity.
- **Answers stop at the ceiling** in practice: the v1 plain-marker template rarely elicits EOG,
  so size `max_tokens` for the answer you want; the display trims at the next marker, and the
  commitment always covers the full executed output.
- **Determinism is the class's**: run the pinned worker on hardware inside the registered class
  or the panel replay will rightly refute the trace.

## Smokes (both run the real model)

```bash
python3 scripts/misaka-palw-fp-v3-worker-smoke.py ./target/release/palw-worker "$MISAKA_PALW_GGUF"
python3 scripts/misaka-palw-fp-gateway-smoke.py ./target/release/misaka-palw-gateway ./target/release/palw-worker "$MISAKA_PALW_GGUF"
```

The worker smoke pins the property everything else stands on: the Text arm and the TokenIds
(replay) arm converge on byte-identical roots, so a panel seat holding only chain data can
re-derive exactly what your gateway committed.
