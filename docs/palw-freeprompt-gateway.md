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
# The worker reads its runtime from the environment — the gateway spawns it, so these three must
# be set where the GATEWAY runs, and as absolute paths: the worker resolves them from its own cwd,
# not from yours. Without them the gateway reports only "the worker exited before announcing its
# manifest" (its stderr is withheld by ADR-0079 SA-7; set MISAKA_PALW_GATEWAY_LOG_WORKER_STDERR=1
# to see the worker's own line, e.g. "MISAKA_PALW_NETWORK_ID is not set").
export MISAKA_PALW_NETWORK_ID=testnet-11
export MISAKA_PALW_ARTIFACT=/abs/path/to/qwen25-a16-graph-v5.palwart
export MISAKA_PALW_TOKENIZER=/abs/path/to/qwen25-tokenizer.json
MISAKA_ROOT=$(git rev-parse --show-toplevel)
"$MISAKA_ROOT"/target/release/misaka-palw-gateway \
  --worker "$MISAKA_ROOT"/target/release/palw-a16-fp-worker \
  --outbox ~/.misaka-palw-outbox \
  --identity /abs/path/to/identity.json \
  --rpc 127.0.0.1:17610 \
  --class-leaves 7708 \
  --bond-exposure-room-sompi 0 \
  --claim-exposure-sompi 0
```

`identity.json`'s `class_id` must be the class the worker's artifact derives — the worker pins
the request's `class_id` to its own and refuses any other ("the request declares a runtime this
worker is not"). `palw-class ledger --network testnet-11` prints it beside the model id.

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
| `--privacy public-da\|panel-da` | ADR-0077 D16: `panel-da` files commitments that carry no prompt on chain — the ids reach only the drawn seats over the authenticated pull. Refused per request where the chain has not armed `palw_panel_da` (`panel_da_armed` in the facts), before the inference. The gateway prints the disclosure sentence at boot: private from the public, not from the panel; a dispute publishes it |
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
  --capture ~/.misaka-palw-outbox/traces/<id>/material.bin \
  --submit --rpc 127.0.0.1:17610
```

In that one step: the anchor is checked against the node's DAA (SA-1b), `<claim>.material` and
`<claim>.answer` (ADR-0084: the job, the prompt ids and the answer's ids, what a seat is served
when the capture is over the transport cap) are staged as `.partial`s in the directory the node
names as its own (`getPalwProducerFacts` → `palwRetentionDir`; `--retention-dir` overrides it, and
a directory the node's panel does not read serves nobody), the transaction is broadcast, and the
files take their real names only after the node accepts. A refusal leaves nothing behind — no
material for a claim that does not exist — and a mempool collision is reported as a wait rather
than a fault.

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

## The OpenAI surface (ADR-0096)

A client written against `api.openai.com` works against this gateway with the base URL changed
and nothing else — and where it cannot, it is told why, by name, before any inference runs.
`GET /v1/models` lists the one class this gateway serves as `misaka-palw-fp-v3` (with the class
id, the manifest's model id, `n_ctx` and the template id under `misaka`); `model` in a request is
echoed, never matched. Every refusal below comes from one function (`surface::admit_request`),
called before the queue is reserved and before the worker is touched, so a refusal is a 400 and
never a spent inference. The conformance corpus in
[`openai-surface/v1/`](openai-surface/v1/README.md) pins every row of this table as a request and
its verdict; the Studio runs the same files against its `/v1`.

| field | what the gateway does |
|---|---|
| `messages[].content` as a list of `{type:"text"}` parts | flattened to one string, parts joined by `\n` |
| `messages[].content` with a non-text part (`image_url`, `input_audio`, `file`, …) | refused by name, with the message and part index |
| `tools`, `tool_choice`, `messages[].tool_calls`, role `tool` | rendered as the model's own text (below) |
| `response_format` (`text` / `json_object` / `json_schema`) | advisory mode (below); a schema outside the subset is refused by name |
| `temperature`, `seed` | ADR-0082 Decision 11, unchanged: anything but greedy is refused naming `palw_fp_decode_rules` while the fence is dormant |
| `top_p`, `top_k`, `min_p`, `repeat_penalty`, `frequency_penalty`, `presence_penalty` | accepted at their identity value only (1, 0, 0, 1, 0, 0) and reported; any other value is refused by name — no consensus rule exists for them, and none will |
| `stop`, `logit_bias` | accepted empty; refused by name otherwise (the display cut is the template's; trim in your app) |
| `n ≠ 1`, `logprobs`, `top_logprobs`, `functions` / `function_call` (legacy), any `stream_options` key but `include_usage` | refused by name |
| `max_completion_tokens` | read as `max_tokens`; both present and different is refused by name |
| `user`, `metadata`, `store`, `parallel_tool_calls`, `messages[].name`, `messages[].tool_call_id`, the ids on replayed `tool_calls` | accepted, no effect, listed in `misaka.ignored_fields` |
| `misaka.require_committed_format: true` | refused by name naming `palw_fp_decode_constraint` until the network arms it |
| `stream_options: {include_usage: true}` | OpenAI's usage chunk before `[DONE]` (the `misaka` event carries the counts anyway) |
| `Authorization: Bearer …` | ignored; the pool slot token is the credential |
| any field not named here | refused by name, never dropped |

**The limits, before the first token (ADR-0097 Decision 2).** `GET /v1/models` carries
`misaka.limits` and `GET /health` carries the same object (`limits`, schema `misaka.palw.limits.v1`):
`context_window` (the class's `n_ctx`; prompt and answer together, `prompt_plus_answer_must_fit`),
`max_output_tokens` (`min(--max-decode-cap, n_ctx − 1)`), `default_output_tokens`,
`max_prompt_bytes`, `tokenizer_id` and `vocab` (count with the table ADR-0096 Decision 8 serves),
`eog_token_ids`, `jobs_per_request: 1`, `streaming: true`, and under `features` the word the chain's
fences decide for each thing the table above accepts — `response_format.json_schema:
"advisory" | "committed"`, `sampling.temperature: "greedy_only" | "requested"`,
`require_committed_format: "refused" | "served"` — and under `privacy` whether the ids ride the
chain (`public_da` / `panel_da_available`) and in which form (`flat` / `merkle`). A client that reads
this budgets its request as arithmetic and never learns the window from a 400.

When a request does exceed a bound the gateway checks before the chain, the 400's `error` string
is exactly what it was (the Studio parses it) and `misaka.refusal` rides beside it with a code:
`context_length_exceeded` carries `prompt_tokens`, `decode_ceiling`, `context_window` and
`room_for_answer`; `prompt_bytes_exceeded` carries `prompt_bytes` and `max_prompt_bytes`. Every
answer also carries `misaka.decode = {requested_max_tokens, applied_limit, cap, clamped}`, because
a `max_tokens` past `--max-decode-cap` is clamped and a clamp nobody is told about is a downgrade
nobody agreed to. Whether a MODEL fits this chain at a width — every wall, with its number — is
`misaka-palw-base0 --bin palw-model-fit` (ADR-0097 Decision 1), not this gateway's to say.

**Tools are the model's own text (Decision 2).** The shipped classes' models were trained on the
Hermes-style convention, and the template is the model's (ADR-0077 Decision 6), so the tool list
rides the system turn as text — created with `You are a helpful assistant.` when the request has
no system turn — in the exact words of Qwen2.5-Instruct's `chat_template`:

```text
# Tools

You may call one or more functions to assist with the user query.

You are provided with function signatures within <tools></tools> XML tags:
<tools>
{"function":{"description":"Get the weather","name":"get_weather","parameters":{…}},"type":"function"}
</tools>

For each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:
<tool_call>
{"name": <function-name>, "arguments": <args-json-object>}
</tool_call>
```

An assistant message carrying `tool_calls` renders as its content followed by one
`<tool_call>\n{"name": "…", "arguments": {…}}\n</tool_call>` per call; a `tool` message renders as
a `user` turn wrapping `<tool_response>\n…\n</tool_response>`, and consecutive tool messages merge
into one user turn. Every JSON object in the prompt is written in RFC 8785 form (sorted keys, no
whitespace) so the prompt — and therefore the job id — is not a function of the client's key
order. The tool block is a `Text` segment like any other user text: SA-3's check is unchanged and
no control token is placed. What changes is the template id: a prompt that spoke the convention
carries `…/chat-segments-tools/v1` (or the think-closed / plain sibling) while `/health` keeps
advertising the base id, because the tools id is a fact about one request and the base id is the
model's. After the run, every well-formed `<tool_call>` block in the shown answer becomes
`choices[0].message.tool_calls[]` in OpenAI's shape (`id: call_…` is a keyed hash of the job id and
the index) with `finish_reason: "tool_calls"`; a malformed block stays in the text and is counted
in `misaka.tool_calls_unparsed`. Parsing reads the display string and changes nothing committed —
the ids, the bytes and the roots are those of the same run. `tool_choice: "required"` or a named
function is ADVISORY: one sentence is appended to the system turn and
`misaka.tool_choice.enforcement` says `advisory`; the round-trip (execute the tool, send the result
back) is the app's, and each leg is its own inference and its own claim.

**`response_format` has two enforcement modes, and the answer says which (Decision 3).** The mode
is the chain's to decide. *Committed* — the network has armed `Params::palw_fp_decode_constraint`
(ADR-0096 Decision 8; Part B of the ADR, not in this tree): the schema compiles to a decode
constraint the seat replays and the court can try. *Advisory* — every shipped network today: the
instruction rides the system turn as text (`Respond with a single JSON value and nothing else.`,
or `… that conforms to this JSON Schema and nothing else:` followed by the schema in RFC 8785
form), the run is unconstrained, and the shown answer — whitespace trimmed, a code fence refused
rather than unwrapped — is parsed as one JSON value and validated after the fact. The schema
subset is `misaka-palw-constraint`'s (draft 2020-12: `type`, `properties`, `required`,
`additionalProperties`, `items`, `minItems`/`maxItems`, `enum`, `const`, `pattern`,
`minLength`/`maxLength`, `minimum`/`maximum`, nesting to 16); `$ref`, `oneOf`, `anyOf`, `allOf`,
`not`, `if`, `format`, `patternProperties`, `dependentRequired` and the rest are refused by name
in both modes rather than approximated. An integration that needs the guarantee sets
`misaka.require_committed_format: true` and is refused by name on a dormant network before any
inference — it never receives a lookalike. `/health` reports `fp_decode_constraint_armed` beside
the other fences.

**What the `misaka` object gained.** Beside the job, claim, roots, `output_token_ids`,
`job_context` and derivation it already carried:

| key | what it says |
|---|---|
| `sampling` | `{requested: {…the knobs as sent…}, applied: {temperature, seed}, reason, not_a_rule_on_this_lane: […]}` — what was asked, beside what ran (Decision 4) |
| `ignored_fields` | the accepted no-effect fields this request sent, by name |
| `tool_choice` | `{requested, enforcement: "advisory"}` when the request declared tools or a choice |
| `tool_calls_unparsed` | `<tool_call>` blocks left in the text because they were not a `{name, arguments}` object or never closed |
| `format` | `{requested: {type, name, constraint_id, constraint_bytes}, enforcement: "advisory", valid, errors: […], canonical_sha256}` — `constraint_id` is `H("misaka-palw/constraint/v1" ‖ RFC 8785 schema bytes)`, `canonical_sha256` the digest of the answer's canonical bytes when valid |
| `answer_untrimmed` | the whole rendering, before any block was lifted out of it |

The same object rides the SSE stream's terminal event; the terminal chunk's `delta` carries the
parsed `tool_calls` (with `index`) and the finish reason.

## An answer-only local engine (ADR-0096 Decision 10)

The gateway is also the local chat engine: the same binary, over the same family worker, filing
nothing. This is what MISAKA Studio's `misaka` engine runs, and it needs no node, no bond and no
key.

```text
identity.json   {}                                        # no bond, no key, no class
anchor.json     {"anchor_block": "<128 hex, not zero>", "anchor_daa": 0}
```

```bash
MISAKA_PALW_NETWORK_ID=testnet-11 \
MISAKA_PALW_ARTIFACT=/abs/qwen25-1.5b-a16.bound.palwart \
MISAKA_PALW_TOKENIZER=/abs/tokenizer.json \
misaka-palw-gateway --listen 127.0.0.1:18899 --worker /abs/palw-a16-fp-worker \
  --outbox /abs/outbox --identity /abs/identity.json --anchor /abs/anchor.json \
  --answer-never-commit --max-decode-cap 512
```

* **The identity may be `{}`** — only with `--answer-never-commit` and `--anchor`. The bond, the
  key, the network domain and the operator read as zeros, and the class id is adopted from the
  worker's manifest after boot (the one value the worker would refuse any other of). `--rpc`
  still needs the class id, because it reads the class's facts by id. `/health` then says
  `bond: null`, `can_submit: false`, and every answer says `committed: false`.
* **The anchor must not be zero** (the gateway refuses a zero anchor as "no anchor available")
  and should not be a real block: the Studio uses
  `blake2b-512("misaka-studio/local-answer-only-anchor/v1")`.
* **The artifact must declare its tokenizer.** The family worker refuses, at boot, an artifact
  whose `tokenizer_commitment` is all zeros — and the published `qwen25-1.5b-a16.palwart`
  (sha256 `a8c4e53e…`) is one (measured 2026-09-10). Bind it once:

  ```bash
  palw-class bind-tokenizer --network testnet-11 --tokenizer /abs/tokenizer.json \
    --out qwen25-1.5b-a16.bound.palwart --model-id 'Qwen/Qwen2.5-1.5B/graph-v5@512' qwen25-1.5b-a16.palwart
  ```

  It sets that one field, reads the result back bound, and prints every row's registered root
  before and after. Measured on the published file: the tiled-map rows (`graph-v2`, `graph-v3`,
  `graph-v5@512`) keep the inventory root `1a7457f1…`; the one-byte-map rows move with the
  digest, and for them the output is a new artifact. The output's sha256 was `3f8fc5066bafae28…` —
  the `bound-candidate.palwart` the testnet-11 fleet runs — so anyone can reproduce the fleet's
  file from the public one and the public `tokenizer.json`, and compare by digest. 41 s, 6.4 GB
  peak memory on an M4 Pro.

Measured end to end on that file: from `{}`, the gateway adopted class `4277d84f…` and answered;
a `response_format: json_object` request returned `{"capital":"Paris"}` in 7 s with
`misaka.format` advisory and valid.

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
# All three run the real worker, which needs MISAKA_PALW_NETWORK_ID / MISAKA_PALW_TOKENIZER in the
# environment and an ABSOLUTE artifact path (the worker resolves paths from its own cwd). The
# gateway smoke takes the artifact, derives the identity's class id from `palw-class ledger`
# (built by `cargo build --release -p misaka-palw-sdk`, expected beside the gateway binary or named
# by PALW_CLASS_BIN) and declares the exposure so the offline form actually writes a commitment.
export MISAKA_PALW_NETWORK_ID=testnet-11
export MISAKA_PALW_TOKENIZER=/abs/path/to/qwen25-tokenizer.json
MISAKA_ROOT=$(git rev-parse --show-toplevel)
python3 scripts/misaka-palw-fp-v3-worker-smoke.py "$MISAKA_ROOT"/target/release/palw-a16-fp-worker "$MISAKA_PALW_GGUF"
python3 scripts/misaka-palw-fp-gateway-smoke.py "$MISAKA_ROOT"/target/release/misaka-palw-gateway "$MISAKA_ROOT"/target/release/palw-a16-fp-worker /abs/path/to/qwen25-a16-graph-v5.palwart
python3 scripts/misaka-palw-fp-rail-smoke.py "$MISAKA_ROOT"/target/release/misaka-palw-gateway "$MISAKA_ROOT"/target/release/misaka-palw-fp-rail "$MISAKA_ROOT"/target/release/palw-a16-fp-worker "$MISAKA_PALW_GGUF"
```

The worker smoke pins the property everything else stands on: the Text arm and the TokenIds
(replay) arm converge on byte-identical roots, so a panel seat holding only chain data can
re-derive exactly what your gateway committed. The gateway smoke adds the two ADR-0077 bindings —
the SSE answer equals the buffered one and `answer_stream_checked` is true (Decision 2 / W5), and
`prompt_ids_checked` is true on the artifact (SA-3) — and reads `/health` for all four chain names.

The unit tests need no model at all: `cargo test -p misaka-palw-gateway -p misaka-palw-fp-submit
-p misaka-palw-constraint` covers the prompt plan, the SA-3 divergence, the W5 mismatch, the
UTF-8-safe stream, the four chain-side refusals, the anchor expiry, the stage/broadcast/rename
ordering, the OpenAI surface's every refusal and the conformance corpus, the tool render and
parse, the schema subset and RFC 8785 (with the RFC's own vectors).
