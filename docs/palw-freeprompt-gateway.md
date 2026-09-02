# The free-prompt gateway — run your own LLM, mine with the same inference (ADR-0044)

Status: FP-06 through FP-09b landed and measured on the pinned model — the gateway, the retained
trace, and the executor rail that signs a real commitment transaction — plus `misaka palw
fp-submit`, which puts that transaction on the chain. ADR-0077 Decisions 1-4 are landed on top:
the worker is resident (`v3-serve`), the answer streams as SSE while the commitment does not, the
gateway reads the four chain facts it commits against, and the rail finishes the handoff through
one library.

**Corrected twice; both corrections are here because a stale "it cannot work" stops the next
person looking.** The 2026-08-31 note removed "no network accepts the free-prompt subnetwork yet":
`tx_validation_in_isolation` validates subnetwork `0x4a`, `calculate_l1_tag` carries the algo-7
arm, and testnet-11 runs the `ConsensusV2` bundle. What it then said still stood — the gateway's
worker was a llama.cpp/GGUF runtime that no chain registers as a class — and ADR-0077 Decision 5
removed that too: the workers behind this gateway are now the FAMILY workers
(`palw-a16-fp-worker`, `palw-qwen36-fp-worker`), which are the runtimes the class rows name. The
question left is no longer "can this reach a chain" but "does the chain certify THIS class", and
that is a thing `/health` answers by name (Decision 3, below).

## What this is

```
your app ──POST /v1/chat/completions──▶ misaka-palw-gateway ──▶ family worker --mode v3-serve
                                        │        │                     │  (mapped ONCE)
                                        │        └──▶ your node    Token frames ──▶ SSE deltas
                                        │             registered/       │
                                        │             fp_certified/     ▼
                                        │             bond_active/  Result frame
                                        │             exposure_room     │
                                   OpenAI-style reply                   │
                                   + roots + work_leaves in-band ◀──────┘
                                              │
                                    outbox artifact (framed result + JSON summary
                                    + the unsigned commitment, if the chain allows one)
```

One inference. The same run that answers you is the run whose commitment can later certify and
mine (ADR-0044). There is no second, mining-only lane anywhere in this path.

## Build

```bash
MISAKA_LLAMA_SRC=$HOME/Downloads/misaka-palw-runtime/llama.cpp cargo build --release -p misaka-palw-worker
cargo build --release -p misaka-palw-gateway -p misaka-palw-derive   # the derive crate builds palw-evm-runner, which code/contract need beside the gateway
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

**The chain (ADR-0077 Decision 3).** Point the gateway at your node with `--rpc <host:port>` (the
same wRPC-borsh endpoint `misaka --rpc` takes) and it reads, per job: the class registry row, the
free-prompt-certified set (`ClassLaneCertified`, genesis ∪ chain), the executor bond and its
exposure room, and a fresh anchor. `/health` names all four — `registered`, `fp_certified`,
`bond_active`, `exposure_room` — and a job on a class the chain does not certify is still
**answered**; only its commitment stays in the outbox, with the reason attached.

`anchor.json` is the OFFLINE form, for drills and rehearsals with no node in reach. It supplies the
freshness binding and nothing else, so the four facts read `unknown` and the gateway cannot submit:

```json
{ "anchor_block": "…128 hex…", "anchor_daa": 123456 }
```

One of `--rpc` or `--anchor` is required. `--rpc` wins if both are given.

## Run

```bash
./target/release/misaka-palw-gateway \
  --worker ./target/release/palw-a16-fp-worker \
  --outbox ~/.misaka-palw-outbox \
  --identity identity.json \
  --rpc 127.0.0.1:17610 \
  --class-leaves 7708 \
  --bond-exposure-room-sompi 0 \
  --claim-exposure-sompi 0
```

**The flags a fleet gateway needs**, and what each one is for:

| flag | what it is |
|---|---|
| `--worker <bin>` | the family worker binary; spawned ONCE as `--mode v3-serve` |
| `--identity <json>` | the bond's executor identity (above) |
| `--outbox <dir>` | where artifacts, unsigned commitments and retained traces go |
| `--rpc <host:port>` | the node whose chain this gateway commits to (Decision 3) |
| `--anchor <json>` | the offline alternative to `--rpc`; cannot submit |
| `--class-leaves <n>` | the class's `pwu_per_inference`, for the quanta display |
| `--bond-exposure-room-sompi <n>` | SA-1: the operator's own ceiling on the loss. `0` = read it from the chain |
| `--claim-exposure-sompi <n>` | what one claim reserves. `0` = read it from the chain |
| `--public-job-budget-permille <n>` | the share of the room strangers' jobs may spend per day (default 200) |
| `--answer-never-commit` | SA-1(c): answer every prompt, commit none |
| `--per-source-jobs-per-window <n>` | SA-8's secondary per-IP quota |
| `--derive-seed <file>` | ADR-0078: sign derivations here. **Must live outside `--identity`'s directory and outside `--outbox`** — the boot refusal scans exactly those two for reachable signing secrets and will refuse to start |
| `MISAKA_PALW_GATEWAY_LOG_WORKER_STDERR=1` | print the worker's stderr. Withheld by default (ADR-0079 SA-7): that stream is the model runtime's and can quote its input |

Without `--bond-exposure-room-sompi`/`--claim-exposure-sompi` AND without `--rpc` the gateway
cannot price the spend, so it answers and commits nothing — the safe reading of an unknown.

Then point any OpenAI-compatible client at it:

```bash
curl -s http://127.0.0.1:8790/v1/chat/completions \
  -H 'content-type: application/json' \
  -d '{"messages":[{"role":"user","content":"What is 2+2? Answer in one short sentence."}],"max_tokens":24}'
```

The reply is a normal chat completion plus a `misaka` object: `fp_job_id`, the three commitment
roots, `work_leaves`, the artifact path, `committed`, and — when it is false —
`not_committed_because` in the chain's own words.

**Streaming (ADR-0077 Decision 2).** `"stream": true` is served as SSE: `chat.completion.chunk`
events as the worker decodes, then one event carrying the `misaka` object and `usage`, then
`[DONE]`. The commitment is NOT streamed; it exists only at completion. When the frame arrives the
gateway re-checks that the ids it streamed are the committed `output_token_ids` and that the bytes
it streamed are the result's own rendering (invariant W5). If they differ the stream closes with an
error event and **no commitment is written** — a worker that shows one answer and commits another
is not the user's inference.

## What the artifact is, and is not

Per job the outbox holds `fp-job-<id>.result.borsh` (the framed `PalwFpWorkerResultV3`),
`fp-job-<id>.commitment-unsigned.borsh` (the assembled commitment, DA trio included),
`fp-job-<id>.json` (the human summary), and `traces/<job-id>/` (the retained event-hash chunks
plus their manifest — written by the worker BEFORE its result frame exists, because a producer
that kept nothing would default in court).

What is still pending before an on-chain commitment lands: the signature and the submission, both
below. The gateway holds NO key (ADR-0079 Decision 4) and therefore does neither.

**The unsigned commitment is written only when the job may become a claim.** ADR-0077 Decision 3
and SA-1/SA-7: an unregistered or uncertified class, an unpriced lane, an inactive bond, no
exposure room, the daily public-job budget, or `--answer-never-commit` each leave the answer intact
and the commitment unwritten, with `not_committed_because` naming which. A derivation
(`"derive": …`) is refused the same way and for a harder reason: consensus rejects a
`DerivedArtifactV1` whose claim never entered the state (`DerivedClaimMissing`), so deriving for an
uncommitted job would produce an object no chain can accept.

**A queued commitment expires with its anchor** (SA-1b). Every job sweeps the outbox and renames
anything older than `commit_by_anchor_daa` to `….expired`; the rail refuses to read through that
name, and the submit path re-checks the anchor against the node's own DAA before it stages or
broadcasts anything. Three places, one rule.

## The executor rail — signing the commitment

```bash
./target/release/misaka-palw-fp-rail --artifact ~/.misaka-palw-outbox/fp-job-<id> --print-claim
```

prints the claim id and the signing purpose (`PalwFpCommitmentV3`) — exactly the digest a
`kaspa-pq-signer` sidecar signs, so a signer-backed rail needs no key in this process. For drills
and devnets the rail can hold the key itself:

```bash
./target/release/misaka-palw-fp-rail --bond-key-seed bond.seed --print-bond-pubkey
```

(put that `executor_pubkey` in `identity.json` before any inference runs — the rail refuses to
sign a job whose commitment names a different key), then:

```bash
./target/release/misaka-palw-fp-rail --artifact ~/.misaka-palw-outbox/fp-job-<id> --bond-key-seed bond.seed --funding-outpoint <txid>:<index> --funding-amount <sompi>
```

writes `fp-job-<id>.commitment-tx.borsh` and a `rail.json` summary carrying the claim id and the
quanta/pwu the job earns. The rail cross-checks the result and the commitment against each other
before signing (`palw_fp_sign_gate`), so an outbox edited in between is refused.

**One handoff (ADR-0077 Decision 4).** Add `--submit --rpc <host:port>` and the same command
finishes the job — it hands the signed transaction to `misaka-palw-fp-submit`, the library
`misaka palw fp-submit` also calls:

```bash
./target/release/misaka-palw-fp-rail --artifact ~/.misaka-palw-outbox/fp-job-<id> \
  --bond-key-seed bond.seed --funding-outpoint <txid>:<index> --funding-amount <sompi> \
  --submit --rpc 127.0.0.1:17610 --retention-dir ~/.misaka/palw-retention
```

In that one step: the anchor is checked against the node's DAA (SA-1b), `<claim>.material` is
staged as a `.partial`, the transaction is broadcast, and the file takes its real name only after
the node accepts. A refusal leaves nothing behind — no material for a claim that does not exist —
and a mempool collision is reported as a wait rather than a fault.

## The worker protocol (ADR-0077 Decisions 1, 2 and 6)

A family worker — `palw-a16-fp-worker` (dense tier) or `palw-qwen36-fp-worker` (hybrid) — has
three modes, and they are the same code from the request onward:

```text
  --mode v3-manifest   the identity, as one JSON line              (map, print, exit)
  --mode v3-job        one framed request in, one result out       (map, run, exit)
  --mode v3-serve      the manifest, then a resident request loop  (map ONCE, then jobs)
```

`v3-serve` is what makes a 33 GiB class usable: the artifact used to be mapped inside `run_job`,
about eight minutes per REQUEST, and the resident mode pays that once. A gateway spawns the worker
with `--mode v3-serve --trace-out <dir>` and keeps its stdin/stdout pipes; both are the v2
length-prefixed framing (four-byte little-endian length, then that many bytes) already used by
`v3-job`.

* **In**: one Borsh `PalwFpWorkerRequestV3` per frame. One generation at a time — a single engine
  and a single KV cache — so the next request is read only after the previous job is answered.
* **Out**: `PalwFpWorkerFrameV1::Manifest` once, first. Then per accepted request, zero or more
  `Token { token_id, rendered }` in decode order and then **exactly one** terminator, `Result` or
  `Refused`. `rendered` is that id's bytes alone: a multi-byte character straddles two tokens, so a
  display buffers an incomplete UTF-8 tail — the pieces concatenated are exactly the result's
  `rendered`, which is what makes the Decision 2 re-render check an identity rather than two
  decoders agreeing.
* **A refused request does not stop the worker.** One bad job must not drop a resident artifact,
  so a refusal is a `Refused` frame and the loop reads the next request. `v3-job` has no `Refused`
  frame: there, a refusal is an empty stdout and a non-zero exit.
* **A job's roots through `v3-serve` are byte-identical to the same job's roots through `v3-job`**
  (invariant W6, pinned by a test on a fixture-sized artifact).

The manifest is the identity a gateway pins its requests with, and it carries what Decision 6
needs: `special_tokens` (every control token by NAME and id) and `eog_token_ids`. A gateway builds
its chat prompt as `PalwFpWorkerInputV3::Segments` — markers as `Special(id)` looked up by name,
the user's text as `Text(bytes)`. The worker emits a `Special` verbatim and encodes every `Text`
segment with special-token parsing disabled, so a user who types the twelve characters
`<|im_start|>` gets twelve characters' worth of ordinary pieces and never the control id.

Two things the manifest states that are easy to get wrong:

* `n_ctx` is the **class's registered** context, not the artifact's rotary span. The dense
  artifact's table covers 512 positions and the class registers 16; the runtime answers at 16,
  because answering wider than the court admits is exactly the split ADR-0077 R0 exists to close.
* `eog_token_ids` is a **display** stop. Execution runs to the job's declared decode budget — a
  step leaf hash binds the job context, which binds the executed count, so hashing cannot start
  before the count is fixed — and the commitment covers every executed token.

The resident worker verifies its artifact by reading all of it at startup and re-verifies whenever
the file's device, inode or size changes (ADR-0077 SA-6). An artifact replaced or truncated under a
running worker is a `Refused` job naming the two digests, never a crash. Nothing the worker logs
carries prompt text or prompt ids: a refusal names the rule and the position it was broken at
(ADR-0079 SA-7).

## Boundaries to know

- **Prompts are public.** PublicDA is the only weight-bearing mode: the committed job carries
  the token ids whole. Do not point private material at a gateway whose outbox feeds a chain.
- **Prompt budget — read this before sizing anything.** `prompt + decode ceiling` must fit the
  CLASS's registered `n_ctx`, and on this build the worker sets both `n_ctx` and
  `prefill_single_batch_cap` from the class row (`fp_worker.rs`), so there is no separate 512-token
  prefill allowance: the class's width is the whole budget. Today the widest registered model class
  is **16 tokens for prompt and answer together** (`QWEN25-A16`; the floor is 12 and `QWEN36` is 8),
  and the ChatML wrapper alone is 8 of them — so `"hello"` leaves 7 decode tokens and a one-sentence
  prompt does not fit at all. Over the width the worker refuses the job by name rather than trimming
  it:

  ```
  prompt 23 + decode ceiling 256 exceeds max_context_tokens 16
  ```

  This bullet used to say "single-batch prefill caps the prompt at 512 tokens", which was true of
  a bundle cap and never of a registered class — an optimistic number is as stale as a pessimistic
  one. Longer rows are a NEW class identity (`n_ctx` is inside the shape profile id), which is the
  ladder ADR-0077 Decision 13 exists for; [testnet11-ask-for-a-file.md](testnet11-ask-for-a-file.md)
  §0 states what today's width means for a person typing a prompt.
- **The display stop is not the execution stop.** On a model whose control tokens the manifest
  declares, the `chat-segments/v1` template elicits EOG and the shown answer ends there. On one it
  cannot name, the `plain-markers-segments/v1` fallback rarely does, so size `max_tokens` for the
  answer you want and the display trims at the next marker. Either way execution runs to the
  declared decode budget — a step leaf hash binds the executed count before the first leaf is
  hashed — and the commitment covers every executed token.
- **The gateway holds no key** (ADR-0079 Decision 4). It refuses to boot if a 32-byte file or a
  seed variable is reachable in its own view of `--identity`'s directory or `--outbox`.
- **Determinism is the class's**: run the pinned worker on hardware inside the registered class
  or the panel replay will rightly refute the trace.

## Smokes (all run the real model)

```bash
python3 scripts/misaka-palw-fp-v3-worker-smoke.py ./target/release/palw-a16-fp-worker "$MISAKA_PALW_GGUF"
python3 scripts/misaka-palw-fp-gateway-smoke.py ./target/release/misaka-palw-gateway ./target/release/palw-a16-fp-worker "$MISAKA_PALW_GGUF"
python3 scripts/misaka-palw-fp-rail-smoke.py ./target/release/misaka-palw-gateway ./target/release/misaka-palw-fp-rail ./target/release/palw-a16-fp-worker "$MISAKA_PALW_GGUF"
```

The worker smoke pins the property everything else stands on: the Text arm and the TokenIds
(replay) arm converge on byte-identical roots, so a panel seat holding only chain data can
re-derive exactly what your gateway committed. The gateway smoke adds the two ADR-0077 bindings —
the SSE answer equals the buffered one and `answer_stream_checked` is true (Decision 2 / W5), and
`prompt_ids_checked` is true on the artifact (SA-3) — and reads `/health` for all four chain names.

The unit tests need no model at all: `cargo test -p misaka-palw-gateway -p misaka-palw-fp-submit`
covers the prompt plan, the SA-3 divergence, the W5 mismatch, the UTF-8-safe stream, the four
chain-side refusals, the anchor expiry, and the stage/broadcast/rename ordering.
