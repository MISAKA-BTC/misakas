# ADR-0096 — The app you already use is the entrance, and the shape of the answer is committed

* Status: PROPOSED 2026-09-10. Part A (the entrance) and Part C (distribution, settings, the
  model-request intake) are host-side and land on `feat/adr-0096-everyday-lane` in both
  repositories (misakas and MISAKA-Studio) as they are built — §9 records what has. Part B (the
  shape of the answer) is a ruleset move behind a fence that is `None` on every preset and, as
  ADR-0082 Decision 11 did, is REFUSED at assembly on a build that cannot carry it; nothing in
  Part B changes a shipped job, a shipped claim or the fingerprint.
* Builds on: [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) (R0: one
  inference, one claim; Decision 2: the answer streams, the commitment does not; Decision 3: the
  gateway reads the chain it commits to; Decision 6: the template speaks the model's own control
  tokens; Decision 13: the 512-token rows; Decision 16: `PanelDa`),
  [0078](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) (Decision 2: the DSL is
  the claim's output, canonicalized by a registered grammar; Decision 6: delivery; Decisions 8 and
  9: the kind table is open), [0082](0082-the-close-is-flat-in-the-context.md) (Decision 11: the
  seeded argmax, and the one fence that arms the sampler; §Part D),
  [0084](0084-the-ids-ride-the-capture-stays-home.md) / [0086](0086-the-opening-carries-the-fold-not-the-leaves.md)
  (what is served, and its cap), [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md)
  (Decision 4: a wider row is a new class at mint, never a raised ceiling),
  [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) (a line, an owner, a
  version — the thing a model request becomes).
* Amends: ADR-0078 Decision 6's rendering premise, by Decision 6 below, **only past the fence of
  Decision 8** — the shipped rendering does not move.
* Supersedes nothing.

## 0. The sentence this ADR is

Everything a person actually does with a small local model — point the app they already use at
it, keep the conversations they had, ask for JSON, let it call a tool, carry a long thread — is
today either refused by the free-prompt lane or done in a way the chain cannot see. This ADR makes
the **entrance** the person's own app, with their history and their tools, and makes the
**shape** of the answer a field of the job the seat replays and the court can try — while moving
none of the constraints the court needs: one inference is still one claim, the prompt still
reaches the model intact, the context is still the class's, and nothing is ever downgraded
silently.

The operator's word for it (2026-09-10): *the base is finished; what is in the way is the
onboarding, the settings, and above all the constraints that come from the fact that this is
mining.* Those constraints are real and most of them are correct. This ADR sorts them into the
ones an entrance can absorb without lying, and the one that has to become consensus.

## 1. What was measured

Every item below was read on `main` at `e65ccf20` (misakas) and `7096533` (MISAKA-Studio) on
2026-09-10, with the line it lives on.

### 1.1 The entrance refuses what a stock client sends

* The gateway's surface is `POST /v1/chat/completions`, `GET /health`, `GET /v1/artifacts/<id>`
  and nothing else (`misaka-palw-gateway/src/main.rs:1561-1563`). Its request parser takes
  `model`, `messages`, `max_tokens`, `stream`, `derive`, `serve_dsl`, `temperature`, `seed`
  (`main.rs:559-594`). A message is `{role, content: String}`; the roles are `system | user |
  assistant` and any other is refused, not dropped (`misaka-palw-gateway/src/wire.rs:129-137`).
* The Studio's `/v1` (`crates/misaka-studio-runtime/src/api/openai.rs:37-41`) serves `models`,
  `chat/completions`, `completions`, and its `ChatMessage` is likewise `{role, content: String}`
  (`backend/mod.rs:41-45`).
* So a client written against `api.openai.com` fails on four ordinary things: `content` as a list
  of parts (every current SDK's multimodal-shaped default), a `tool` turn or a `tools` list, a
  `response_format`, and a `temperature` — which the gateway refuses BY NAME while
  `Params::palw_fp_decode_rules` is dormant (`main.rs`, `sampling_from_request`; the fence is
  `None` on every preset and its arming is refused by `validate_palw_v2` on this build,
  `consensus/core/src/config/params.rs:1362`, `:1993-2003`). The refusal is right — ADR-0082
  Decision 11's reason stands — and the client still gets a 400 for a field its SDK set by
  default.
* The Studio does not forward sampling knobs to the gateway at all
  (`backend/gateway.rs:195-199`), so through the app the same request works; the app just never
  says what it dropped.

### 1.2 Conversations live in the window

`ui/src/store/studio.ts:3-10`: conversations are the ONLY thing the UI persists, and they persist
in the WebView's `localStorage` through `zustand/persist`. There is no API that lists them, no
file a person can copy, no import. A person moving from another local app arrives with nothing,
and a person who reinstalls leaves with nothing. The runtime's provenance log
(`records.rs`) is the opposite shape — an append-only JSONL with hashes and, opt-in, the text —
and it is written per completion, not per conversation.

### 1.3 Four tables say what to install, and one of them names a binary that no longer exists

* `PalwClassSpec.artifact` (`crates/misaka-studio-core/src/palw.rs:39-70`,
  `PalwArtifactSource::{DerivedFromSeed, Download{sha256,size,hf_repo}, ConvertLocally}`) pins the
  class artifacts; `spawn_default_class_install` (`state.rs:194`) installs the default one.
* `resolve_program` is spelled twice (`backend/llamacpp.rs:63`, `backend/misaka.rs:79`: beside
  the executable, then `engines/`, then `PATH`); `resolve_kaspad` / `resolve_misaka_cli`
  (`node.rs:793`, `:818`) are spelled a third and fourth time with a different search order (no
  `engines/`).
* The desktop bundle stages `resources/*` (`desktop/src-tauri/tauri.conf.json:26`) and nothing
  checks, at build time, that what it stages is what the runtime will look for — the 2026-09-06
  "No UI bundle" release and the v0.1.0 unreachable-sidecar release were both this.
* And `backend/misaka.rs:1-6` drives `misaka-palw-serve`, which the node tree **deleted on
  2026-09-02** (`2f688bc1`: "one runtime — `v3-serve` on both family workers, `misaka-palw-serve`
  retired", ADR-0077 Decision 1). A component that one repository names and the other no longer
  builds, and nothing in either repository's CI knew.

### 1.4 The settings a panel shows are not the settings the engine was built from

`apply_settings` (`state.rs:267-292`) rebuilds the engine when a hand-kept list of fields
changes. On 2026-09-05 that list lacked the gateway URL and the pool token, so "Forget slot →
Join" left the chat mining on the slot the person had just left, with that slot's token, while
every status panel named the new one (Studio `ceee727`). The fix added two fields to the list.
The list is still a list. `/api/v1/runtime` (`state.rs:48-59`) reports the engine's own view of
what it loaded; nothing reports, beside it, what the running engine was constructed FROM, so a
panel cannot say "the file says X, the engine holds Y".

### 1.5 Adding a model has a path and no door

The path exists end to end — the SDK (`docs/palw-model-onboarding-sdk.md`), conversion,
`--palw-register-class` as a producer-start phase (the Studio's Network tab does this),
certification as a consensus object (ADR-0075, `docs/palw-certify-a-new-model.md`), and a line
with an owner and versions (ADR-0088). What does not exist is the place a person who is not going
to do any of that ASKS: `.github/ISSUE_TEMPLATE/` holds a bug report and a feature request. A
model request is neither, and the questions it has to answer (weights, licence, architecture,
quantization, context, which lane, who converts, who bonds the line) are not in either form.

### 1.6 The context is 512 and the answer is what is left of it — and the 256 wall is gone

* The row is 512 (ADR-0077 Decision 13); the budget is `prompt + decode ceiling ≤ n_ctx`
  (`misaka-palw-base0/src/fp_worker.rs:745-749`), so what a person can get back is
  `512 − prompt`. A ChatML frame with a system line costs on the order of ten tokens; a one-shot
  DSL exemplar costs ~130 (the 2026-09-03 drill's `prompt 134`). ADR-0092 Decision 4 says why the
  number does not move on a running chain: the row's history dissection is what eats the court's
  clock, and a wider row is a NEW class at mint, not a raised ceiling.
* The Studio trims history to fit (`backend/mod.rs:216`, `fit_messages_to_budget`: the system
  prompt and the newest turn survive, oldest turns go first) and says how many turns it dropped.
* The 256-token decode wall of 2026-09-05 (the producer hardcoded one retained-trace chunk;
  `PALW_FP_TRACE_CHUNK_EVENTS_V3 = 256`, `palw_freeprompt_v3.rs:151`) is **fixed on `main`**:
  `0001f34c` derives the free-prompt manifest from the trace's own leaves, chunked at 256
  (`misaka-palw-base0/src/backend.rs:406-408`), and the Studio measured decode 257/257 committed
  (`backend/gateway.rs:238-243`). The remaining ceiling is the row and `--max-decode-cap`
  (hard maximum 4,096, `main.rs:114`).

### 1.7 The shape of the answer is nobody's

* Every backend selects with `base0_decode_token_select_v1` — the plain argmax
  (`qwen25_a16_backend.rs:341`, `:355`; `palw_step_refute.rs:3285`). ADR-0082 Decision 11's
  `decode_token_select_v2` (the seeded argmax, `palw_decode_select_v2.rs:185`) exists in
  consensus-core and in no engine, and the fence that would select it is dormant.
* ADR-0078 gives seven kinds (Decision 8) whose grammars canonicalize the answer AFTER the fact
  and, by Decision 2, change nothing about how it was produced: "nothing about the prompt or the
  answer is changed to make it parse". There is no `json` kind.
* So "give me valid JSON" is a request the lane can only hope to honour. On a 1.5 B model with a
  few hundred tokens of room that hope is measured in the derivation notes of 2026-09-04: a
  derivation succeeded only when `max_tokens` happened to equal the answer's exact length, because
  the model keeps generating past its own end-of-generation token to the declared budget
  (ADR-0077: the commitment covers every executed token) and the derivation reads the whole
  rendering (ADR-0078 Decision 6).

The operator's ordering of these is the right one: the everyday use of a small model is
*hand it a simple task → get the result in a shape (JSON, XML) → store it → use it*. Format is
not a nicety on that path; it is the path. And it is the one item on the list that an entrance
cannot absorb honestly, because the seat replays the decode and a mask the seat does not apply is
a token the seat will not reproduce.

## 2. The requirement

R0, F1 and the refusal doctrine are kept whole: one inference is one claim (ADR-0077 R0); the
prompt reaches the model as the person wrote it (ADR-0044 F1); a request the lane cannot honour
is refused BY NAME before the inference, and nothing is downgraded silently (ADR-0082 Decision
11's rationale, kept as a rule of this ADR). Consensus moves only by activation
(ADR-0065/0066's doctrine; regenesis is not on the table).

Under those: a person's existing OpenAI-shaped app must work against the Studio and the gateway
with the base URL changed and nothing else; their conversations must be theirs (a file, an API,
an import); a long thread must be servable as several claims with the seams visible; a request
for a shape must be either committed — replayed by the seat, triable by the court — or clearly
marked advisory; every component a person needs must come from one manifest that both
repositories publish and both check; every settings panel must show what the running process was
built from; and a model that does not exist yet must have a door to knock on.

## 3. Decisions

### Part A — the entrance (host-side; no consensus object moves)

**Decision 1 — one OpenAI surface, spelled once and served twice.** The Studio's `/v1` and the
gateway's `/v1/chat/completions` accept the same request shape and return the same envelope; the
shape is this section, and a conformance corpus (`docs/openai-surface/v1/*.json` in misakas,
mirrored into the Studio's tests by a script that asserts the copies are identical) pins it in
both trees. Accepted, and what happens to each:

| field | Studio `/v1` | gateway `/v1/chat/completions` |
|---|---|---|
| `messages[].content` as a list of `{type:"text"}` parts | flattened to one string, parts joined by `\n` | same |
| `messages[].content` with a non-text part (`image_url`, `input_audio`, …) | refused by name | refused by name |
| `tools`, `tool_choice`, `messages[].tool_calls`, role `tool` | Decision 2 | Decision 2 |
| `response_format` | Decision 3 / Part B | Decision 3 / Part B |
| `temperature`, `top_p`, `top_k`, `min_p`, `repeat_penalty`, `seed`, `stop` | the engine's | Decision 4 |
| `n ≠ 1`, `logprobs`, `functions` / `function_call` (legacy), `stream_options` other than `include_usage` | refused by name | refused by name |
| `Authorization: Bearer …` | the Studio's own key when one is set | ignored; the pool slot token (`x-pool-token`) is the credential |
| `GET /v1/models` | the models on disk | the one class this gateway serves, as `misaka-palw-fp-v3` plus the class name |

The envelope gains nothing OpenAI does not have except the `misaka` object every gateway answer
already carries (job, claim, roots, `output_token_ids`, `job_context`, derivation), which Decisions
2–5 extend. A field this table does not name is refused by name, never dropped — with two
deliberate exceptions, both reported rather than silent: a sampling knob at its IDENTITY value
(`top_p: 1`, `top_k: 0`, `min_p: 0`, `repeat_penalty: 1`, `frequency_penalty: 0`,
`presence_penalty: 0`, `temperature: 0`), which a stock SDK sends by default and which asks for
nothing, is accepted and listed in `misaka.sampling.requested`; and the fields OpenAI defines as
having no effect on the answer (`user`, `metadata`, `store`, `parallel_tool_calls`,
`max_completion_tokens` as an alias of `max_tokens`) are accepted and listed in
`misaka.ignored_fields`. A knob at any other value is refused by name on the gateway (Decision 4).

**Decision 2 — a tool call is a turn of text; the round-trip is the app's; the schema is the
model's own template.** The lane's chat template is the model's (ADR-0077 Decision 6), and the
shipped classes' models (Qwen2.5-Instruct, Qwen3.x) were trained on the Hermes-style tool
convention: the tool list rides the SYSTEM turn as `<tools>…</tools>` JSON, a call is emitted as a
`<tool_call>{…}</tool_call>` block, and a tool's reply is a `user`-role turn wrapping
`<tool_response>…</tool_response>`. So:

* `tools` are rendered into the system segment as the model's own text (a `Text` segment —
  SA-3's subsequence check is unchanged, the ids are the prompt's); a request with `tools` and no
  system turn gets one that holds only the tool block.
* A `tool` message is rendered as the model's tool-response turn; an assistant message carrying
  `tool_calls` is rendered as the `<tool_call>` block it produced. Both are text.
* The committed answer is parsed for `<tool_call>` blocks after the run; each becomes
  `choices[0].message.tool_calls[]` in OpenAI's shape with `finish_reason: "tool_calls"`, and the
  raw text stays in `misaka.answer_untrimmed`. Parsing changes nothing committed.
* The round-trip — the app executes the tool and sends the result back — is one job per leg.
  Every leg is its own inference and its own claim (R0), reported in `misaka.jobs[]` when the
  entrance drove more than one (Decision 5). Nothing on chain knows what a tool is.
* Under Part B a `tool_choice: {type:"function", function:{name}}` or `strict: true` on a tool's
  parameters compiles to a decode constraint (the call's JSON must match the parameters' schema);
  without Part B it is advisory and says so (Decision 3).

Template ids move: `misaka-palw/fp-gateway-template/chat-segments-tools/v1` (and the
think-closed sibling) name the render with a tool block, so a `/health` that advertises a template
id still advertises the executed one, and the golden tests for the plain and ChatML renders are
untouched.

**Decision 3 — `response_format` is served in two enforcement modes, and the answer says which.**
`{"type":"json_object"}` and `{"type":"json_schema","json_schema":{schema}}` are accepted at both
entrances. The mode is the CHAIN's to decide, not the request's:

* **committed** — the network has armed Decision 8's fence: the schema compiles to a decode
  constraint (Decision 7), the job carries its id, the seat replays it, the court can try it.
* **advisory** — every shipped network today: the schema is rendered into the prompt as text
  (the person asked for it; F1 is intact), the run is unconstrained, the answer is validated
  against the schema after the fact, and the `json` kind (Decision 9) is derived when it parses.

Both modes report `misaka.format = { requested, enforcement: "committed" | "advisory",
valid: bool, errors: [...] }`. A request that sets `misaka.require_committed_format: true` on a
network whose fence is dormant is refused by name BEFORE the inference — an integration that
needs the guarantee must never receive a lookalike. `json_object` compiles to the grammar of any
JSON value; `json_schema` to Decision 7's subset, and a schema outside the subset is refused by
name in both modes rather than approximated.

**Decision 4 — sampling at the entrance: refused by the gateway, mapped WITH NOTICE by the
person's own app.** The gateway keeps ADR-0082 Decision 11's refusal exactly: a non-greedy
`temperature` or a non-zero `seed` is a 400 naming the fence, before the inference. The Studio is
the person's own app, and its chat already sends the lane no sampling knobs at all
(`backend/gateway.rs:195-199`). It now SAYS so: every answer that went to the lane carries
`misaka.sampling = { requested: {…}, applied: { temperature: 0, seed: "00…00" }, reason:
"palw_fp_decode_rules is not armed on <network>" }` and the UI shows it under the message. A
setting `node.sampling_policy` chooses `greedy_with_notice` (the default: the request goes
through, the notice is shown) or `refuse` (the app refuses like the gateway). `top_p`, `top_k`,
`min_p` and `repeat_penalty` have no consensus rule and never will under this ADR — a per-lane
key is the only sampler an exact court can carry (ADR-0082 Decision 11) — and are reported as
`not_a_rule_on_this_lane`. The doctrine is honoured: nobody is told a false thing about what ran,
because what ran is printed beside what was asked.

**Decision 5 — a long thread is a chain of jobs, and the chain is reported.** The row does not
move (ADR-0092 Decision 4; a wider class is a mint-time decision this ADR does not take). The
entrance does three things, each visible:

1. **Trim**, as today: oldest turns first, system prompt and newest turn kept, the count of
   dropped turns reported (`misaka.context = { n_ctx, prompt_tokens, dropped_turns }`).
2. **Summarize** when a trim would drop more than `summarize_after_turns` (default 4): the
   dropped turns are sent as their own job — a prompt asking the class for a short summary — and
   the summary rides the next prompt as a system-level "Earlier in this conversation: …" text
   turn. The summary is an inference and a claim like any other (R0). It is never fabricated by
   the app.
3. **Continue** when `finish_reason == "length"` and the request's `max_tokens` exceeded the
   room: a follow-up job whose prompt is the trimmed context plus the answer so far, asking the
   model to continue; at most `continue_max_legs` (default 2) legs. Each leg is a claim.

Every job an entrance drove for one request is listed in `misaka.jobs[] = [{fp_job_id,
fp_claim_id, role: "summary" | "answer" | "continue" | "tool_leg", prompt_tokens,
decode_tokens}]`, and the record store (Decision 12) keeps the same list on the record. A
KV-cache handoff between jobs is deliberately NOT a mechanism here: a checkpoint is court
material (ADR-0082 Decision 4), not a transport, and the seat replays a job from its prompt ids.

### Part B — the shape of the answer (a ruleset move, behind one fence)

**Decision 6 — the answer is cut at the first committed end-of-generation id, by a versioned
rule.** Past Decision 8's fence, `render_answer_v2` renders the ids up to and excluding the first
id in the manifest's `eog_token_ids`; the ids after it stay executed, committed and priced
(ADR-0077: the execution runs to the declared budget and the commitment covers every token) but
are not the answer, and the derivation (ADR-0078) reads the cut rendering. ADR-0078 Decision 6's
premise — "a trimmed answer is one a verifier holding the ids cannot reach" — is true of a display
trim and false of an EOG cut: the manifest declares the ids, so a verifier holding the ids and
the class reaches the same cut. This is what makes a derived JSON the answer rather than the
answer followed by whatever the model said to an imaginary next turn. The shipped rendering does
not move: `render_answer_v1` stays what every dormant network verifies.

**Decision 7 — a job may carry a decode constraint, and the committed token is the admitted
argmax.** The free-prompt job gains one field, `constraint_id: Hash64` (`0` = none), inside
`fp_job_id_v3` by construction and therefore inside the claim id: a constraint cannot be changed
after the fact, and grinding one costs an inference (ADR-0072 kept). `PALW_FP_V3_VERSION`
moves 5 → 6 WITH the fence, so every v5 job stays exactly what it is.

* `constraint_id = H("misaka-palw/constraint/v1" ‖ canonical constraint bytes)`. The bytes are a
  byte-level automaton in one pinned form: the compiled output of `misaka-palw-constraint`'s
  JSON-Schema subset (draft 2020-12: `type` in object/array/string/number/integer/boolean/null,
  `properties`, `required`, `additionalProperties: false`, `items`, `minItems`/`maxItems`,
  `enum`, `const`, `pattern` over a regex subset the crate pins by table, `minLength`/`maxLength`,
  nesting to a pinned depth) or of a GBNF subset. The compiler is a pure function and is
  content-named like a transformer (ADR-0078 Decision 3): `compiler_id` is in the header of the
  bytes. Bytes are bounded (64 KiB) and refused above it.
* The selection rule v3: at position `p` with automaton state `s = A(bytes(prefix ids))`,
  `committed = argmax_j key_v2(j)` over the lanes `j` with `admit(s, bytes(j))` true, where
  `admit` is "feeding the token's bytes from `s` reaches a non-dead state". If no lane is
  admitted, the committed token is the lowest EOG id and the run's stop reason is
  `EndOfGeneration` — which the validators already accept (`decode_tokens_executed < limit`).
  With `constraint_id = 0`, `admit` is identically true and the rule is `decode_token_select_v2`
  byte for byte, which at `T_q = 0` is `base0_decode_token_select_v1` byte for byte
  (ADR-0082 Decision 11's own reduction, kept).
* The refutation keeps its two-disclosure form and gains a state: the pin already carries the
  prefix ids (`PalwTiledDecodePinV1::generated_token_ids`); the court recomputes `s` by running
  the automaton over the rendered prefix — which needs the class's token-to-bytes table
  (Decision 8) — and applies `admit` to the committed lane and the beating lane before comparing
  keys. A third arm is one disclosure: **the committed token at `p` is not admitted at `p`**,
  proven from the ids and the constraint alone, no tile opened. `check_tiled_decode_token_refutation_v3`
  takes the constraint as the v2 form takes the sampler — bound to the claim by the caller, never
  stated by the challenger (a challenger who could state the constraint could state "none" and
  convict an honest masked token).
* The constraint bytes ride the served answer envelope (ADR-0084's `FPA1`, in its next version)
  under the same data-availability obligation as the prompt ids; the commitment carries only the
  id. Privacy is the prompt's: a schema is not the prompt, but it is as public as the prompt's
  ids are on the network the job runs on (PublicDa today; ADR-0077 Decision 16 unchanged), and a
  schema that names what the person is extracting says something about the prompt. The entrance
  says this beside the `PanelDa` sentence it already shows.
* Priced like every other job field. Decode leaves are what they are (ADR-0082 Decision 10);
  a mask does not change a leaf.

**Decision 8 — one fence, refused at assembly until the build carries all three halves.**
`Params::palw_fp_decode_constraint: Option<ForkActivation>`, top level, `None` on every
preset, in `consensus_identity_id` like its siblings. Armed, it selects: job version 6 with
`constraint_id`; selection rule v3 in the transition's replay and in every engine; refutation v3
in the court; `render_answer_v2` (Decision 6); and the class token-to-bytes table as served
material (the table is what the artifact's `tokenizer_commitment` names — v6 classes serve it
beside the artifact, and a seat that does not hold it files `Unavailable`, which is an abstention
and not a verdict, ADR-0065 D4). `validate_palw_v2` refuses a scheduled height on a build that
lacks any of the three halves, in the sentence ADR-0082 Decision 11's refusal uses
(`params.rs:1993-2003`), because a fence that reads as armed while the transition refuses is a
lane that burns inferences. `GetPalwProducerFactsResponse` gains `fp_decode_constraint_armed`
(one wire-version step, fail-closed like `fp_decode_rules_armed`), `ChainFacts` mirrors it, and
Decision 3's mode reads it. Arming does NOT require `palw_fp_decode_rules`: greedy under a
constraint is a complete rule.

**Decision 9 — the `json` kind (ADR-0078 Decision 8, row 8).**

| kind | the DSL | transformer | artifact | determinism basis | not covered |
|---|---|---|---|---|---|
| `json` | the answer's JSON text | `json/canonical/v1`: RFC 8785 (JCS) canonicalization — sorted member names, shortest round-trip number form, UTF-8, no insignificant whitespace — and identity over the result | `.json` | a pure byte function; the artifact IS the canonical bytes | semantic validation; numbers outside JSON's canonical form; comments and trailing commas (refused, not repaired) |

`grammar_id` for the kind is `json/v1`; when the job carried a constraint, the derivation's
`grammar_id` is `H(json/v1 ‖ constraint_id)` so a consumer checks both that the bytes are
canonical JSON and that they were produced under the schema the claim committed (Decision 7). A
parse failure derives nothing and the inference still certifies and mines (ADR-0078 Decision 2).
`xml` is not a row: an XML canonicalization that a 1.5 B model reliably targets in a few hundred
tokens was not measured, and a kind that is not there is not covered.

### Part C — distribution, settings, the door

**Decision 10 — one components manifest, published by both releases, read by every installer.**
`components.json` (schema `misaka/components/v1`):

```text
{ "schema": "misaka/components/v1", "release": "<tag>", "network": "testnet-11",
  "components": [ { "id": "kaspad", "kind": "node", "version": "…",
                    "platform": "aarch64-apple-darwin", "url": "…", "sha256": "…", "size": …,
                    "requires": [] },
                  { "id": "palw-a16-fp-worker", "kind": "worker", … },
                  { "id": "misaka-palw-gateway", "kind": "gateway", … },
                  { "id": "misaka", "kind": "cli", … },
                  { "id": "qwen25-1.5b-a16", "kind": "artifact", "class_id": "…",
                    "artifact_root": "…", "url": "hf://…", "sha256": "…", "size": … },
                  { "id": "qwen25-tokenizer-table", "kind": "tokenizer-table",
                    "tokenizer_commitment": "…", … } ] }
```

* The node repository's deploy workflow writes the manifest for the binaries it builds, per
  platform, beside the release assets. The Studio's release workflow writes its own manifest
  (`misaka-studiod`, the shell, the engines it stages) and NAMES the node manifest it was built
  against by URL and sha256. Two halves, one schema, and the reference is the seam.
* The Studio reads the manifest at `/api/v1/components` — installed version, manifest version,
  sha256 verified or not, where it was found (`beside the executable`, `engines/`, `PATH`,
  `models_dir`) — and installs or updates a component from it through the existing download
  manager (which already verifies sha256 and resumes). `misaka-studiod --check` prints the same
  table. `PalwClassSpec.artifact` becomes the OFFLINE copy of the manifest's artifact rows,
  pinned to the same sha256 by a test.
* One resolution rule, spelled once: `resolve_component(id)` replaces the four spellings in
  §1.3 — configured path, beside the executable, `engines/`, `models_dir` (artifacts), `PATH` —
  and every spawn site says which candidate won, in the `effective` view (Decision 11).
* The cross-repository check that would have caught §1.3's last bullet: the Studio's CI fetches
  the node manifest its own manifest names and asserts that every `engine`, `worker`, `gateway`
  and `node` id the Studio can spawn is a row in it. A binary the node stopped building fails the
  Studio's build, not the person's evening.
* The Studio's local integer engine is **the gateway in `--answer-never-commit` mode over the
  family worker** (ADR-0077 Decision 1: the server is the worker; there is no chat-only server
  and there will not be one). A bond-less identity — the class, the network, an executor key,
  no bond — is accepted by the gateway when and only when `--answer-never-commit` is set, and
  `/health` says `can_submit: false` and why. The Studio's `misaka` backend drives that and
  reports the gateway's `template_id` and `n_ctx` as its descriptor. `misaka-palw-serve` is
  removed from the Studio's settings, with a migration that rewrites `backend.misaka_serve_path`
  into the two paths the gateway needs and says so once.

**Decision 11 — every settings panel shows the effective value, and the rebuild predicate is
a fingerprint.** `GET /api/v1/settings/effective` returns, per subsystem (`backend`, `node`,
`records`, `catalog`, `pool`, `gateway`), `{ configured, effective, source, since }` where
`effective` is read from the RUNNING object — the engine's program path, URL and token
fingerprint, the model it holds and its `n_ctx`; the node's argument list; the record store's
path and enabled flag; the catalog endpoint — and `source` says which file, flag, environment
variable or discovery produced it. The UI's Settings and Network views render `effective` and
mark a field whose `configured` differs. Each backend derives a `RuntimeFingerprint` from
everything it copied at construction; `apply_settings` rebuilds when the fingerprint the new
settings would build differs from the running one, so the predicate cannot forget a field because
it no longer enumerates fields. The `Arc::ptr_eq` test of 2026-09-05 stays and gains the
converse: a change to any constructor input replaces the instance.

**Decision 12 — conversations are the runtime's, and the record remembers the lane.**
`/api/v1/conversations` (list, get, put, delete, `export`, `import`) over a JSON file per
conversation under the data directory (`conversations/<id>.json`, the UI's own `Conversation`
shape with `messages[].mining` kept). The UI persists through the API and keeps `localStorage`
as a cache for the window; on first run after the change the cache is imported once. `import`
accepts the Studio's export, OpenAI's account export (`conversations.json`: the `mapping` tree
walked along `current_node`), and a generic `{title?, messages:[{role, content}]}` list; every
imported message carries `source` and the import reports what it skipped by name (non-text parts,
unknown roles). A conversation may change model — a history made with a GGUF continues on a
class, trimmed by Decision 5 and told so. The inference record (`records.rs`) gains
`misaka.jobs[]`, `misaka.format` and the derived `json` artifact's hash when there is one, so
the everyday pipeline — task → shape → store → use — leaves a record a person can `jq`.

**Decision 13 — a model request has a door, and the door is not chain state.** A GitHub issue
form (`.github/ISSUE_TEMPLATE/model-request.yml`) in the node repository asks the questions §1.5
lists; `docs/model-requests.md` describes the pipeline a request goes through — conversion, a
class id, registration, certification, a line with an owner (ADR-0088), a seeded pair (ADR-0090)
— and who does which step; the Studio's Network tab gets "Request a model", which opens the form
prefilled with what the machine knows (RAM, accelerator, the classes it holds). Requests are
public and the queue is the tracker. A request is not a bond, buys no priority and moves no
consensus object; the day one becomes a line, the line's owner is whoever bonded it (ADR-0088),
not whoever asked.

### Part D — what is deliberately not decided

* The width of the next class row (ADR-0092 Decision 4: mint-time, the operator's).
* The heights at which `palw_fp_decode_constraint` arms on testnet-11 (operator; after §6 step
  4 is measured on devnet).
* The regex subset's exact bounds and the constraint compiler's worst-case size: pinned by
  table when the crate lands, and this ADR is amended with the measurement.
* `xml` as a kind (Decision 9's last sentence).
* Whether Decision 5's summary job should be a canonical-prompt claim rather than a user-prompt
  claim (ADR-0074 Decision 1): it is a user-prompt claim here because the text is the person's.

## 4. What this costs, stated before it is measured

* **Court**: recomputing `s` is one automaton step per byte of the rendered prefix (≤ a few
  thousand bytes at n_ctx 512) plus two `admit` checks of one token each — negligible beside the
  tile openings the v2 refutation already pays. The third arm (not admitted) opens no tile.
* **Producer**: per position, `admit` over the vocabulary — 151,936 lanes × the token's bytes
  (mean ~3) ≈ 4.5 × 10⁵ automaton steps, a few milliseconds on the a16 host against ~1 s/token
  of decode. A state-indexed lane cache (llama.cpp's grammar sampler's trick) brings it under
  1 ms. **Measured 2026-09-10 (§9): about 12 ms per position** in release over a synthetic
  151,936-entry table (685,279 bytes, about 18 ns per byte), about 15 ms through a per-lane
  lookup closure — four times the estimate above, and still about 1 % of a ~1 s decode step.
  The lane cache is the engine's work and is not optional at a larger vocabulary.
* **Data availability**: ≤ 64 KiB per constrained claim in the answer envelope; the tokenizer
  table once per class (≈ 3 MB for the Qwen vocabulary), served like the artifact.
* **Wire**: one `GetPalwProducerFactsResponse` version step; the job's borsh gains 64 bytes (a
  `Hash64`), and only at version 6.
* **Nothing on a shipped network moves**: version 5 jobs, their ids, their claims, the
  fingerprint, the rendering rule.

## 5. Invariants the tests must hold

1. `constraint_id = 0` selects exactly what `decode_token_select_v2` selects on every row, and
   at `T_q = 0` exactly what `base0_decode_token_select_v1` selects (a sweep over random rows,
   ADR-0082's `the_greedy_temperature_is_the_shipped_rule_on_every_row` extended).
2. For every constrained run, every committed token is admitted at its position, and the
   third refutation arm convicts a run whose token at any position is not (a negative control
   that alters one id and is convicted).
3. The two-disclosure refutation under a constraint convicts a producer that committed an
   admitted lane when an admitted lane with a strictly greater key existed, and refuses to
   convict when the beating lane is not admitted (the challenger's lane must be admitted).
4. The automaton's `admit` is a function of `(constraint bytes, prefix ids, class token table)`
   only — a test runs it on two hosts' serializations and pins the state sequence.
5. `render_answer_v2` cuts at the first EOG id and only there; `render_answer_v1` is byte for
   byte what it is today; the derivation under v2 of an answer with a trailing tail equals the
   derivation under v1 of the same answer with `max_tokens` at the answer's exact length (the
   2026-09-04 measurement, now a fixture).
6. A `response_format` request on a dormant network returns `enforcement: "advisory"`, and with
   `require_committed_format: true` is refused before any worker is spawned (a mock worker that
   panics on spawn proves "before").
7. The OpenAI conformance corpus passes on the Studio's `/v1` and on the gateway's, from the
   same files, and the copies in both trees are identical (a test in each tree hashes its copy;
   the ADR pins the hash in §9).
8. A `tool` turn renders as the model's tool-response turn and SA-3's subsequence check still
   holds; a `<tool_call>` block in the committed answer parses to `tool_calls[]` and the
   committed ids are unchanged by parsing (the roots of a run with and without parsing are
   equal — it is the same run).
9. `apply_settings` replaces the engine on any change to any constructor input and keeps it on
   a change to none (a table over every settings field, generated from the struct, not written).
10. The Studio's manifest test fetches (or, offline, reads the pinned copy of) the node manifest
    it names and finds every spawnable component id in it; deleting a row fails the test.
11. The four resolution spellings are gone: `grep` for a second `fn resolve_` over the
    runtime is a test.
12. An import of OpenAI's `conversations.json` fixture yields the conversations along
    `current_node`, with non-text parts skipped by name and counted.

## 6. Order of work

1. **Part A on both trees** (this branch pair): Decision 1's parser and corpus; Decision 2's
   template ids, render and parse; Decision 3's advisory mode with `misaka.format`; Decision 4's
   notice; Decision 5's jobs list, summary and continue legs. No consensus object moves.
2. **Part C**: Decision 10's manifest schema and the node workflow's writer; the Studio's
   `/api/v1/components`, `resolve_component`, the cross-repository check, and the `misaka`
   backend over the gateway; Decision 11's `effective` view and fingerprint; Decision 12's
   conversations API and import; Decision 13's issue form, page and button.
3. **Part B, consensus-core first**: `misaka-palw-constraint` (the compiler, pure, content-named)
   and the automaton in consensus-core; selection rule v3; refutation v3 with the third arm;
   `render_answer_v2`; job v6 behind the fence; `validate_palw_v2`'s refusal; the RPC flag.
   Invariants 1–5 green with negative controls.
4. **Part B, engines**: the a16 and qwen36 backends mask under a constraint; the tokenizer table
   as served material; the devnet drill (`--palw-model-devnet` and a scheduled
   `palw_fp_decode_constraint`) commits a constrained claim, replays it on six nodes, and the
   court convicts the two negative controls. §4's costs measured and written into §9.
5. **testnet-11**: heights are the operator's (Part D); the fence is scheduled by activation.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0077 R0 / Decision 2 / Decision 6 | kept; Decision 2 renders tool turns as text under the same template discipline |
| ADR-0078 Decision 2 (nothing about the answer is changed to make it parse) | kept for the advisory mode and for every dormant network; under Decision 8's fence the CONSTRAINT is part of the job, so the answer was never unconstrained — nothing is changed after the fact |
| ADR-0078 Decision 6 (delivery reads the full committed rendering) | amended past the fence by Decision 6 (EOG cut, versioned); untouched before it |
| ADR-0078 Decisions 8/9 (kind table, open) | row 8 `json` added (Decision 9) |
| ADR-0082 Decision 11 (seeded argmax; refuse by name while dormant) | kept whole; Decision 7 composes with it (mask, then key) and reduces to it at `constraint_id = 0` |
| ADR-0084 / 0086 (served material, its cap) | the constraint bytes and the tokenizer table ride the same obligation; the cap is not raised |
| ADR-0092 Decision 4 (width is a mint-time class) | honoured; Decision 5 lives inside the row |
| ADR-0088 / 0090 (lines, seeds) | Decision 13 routes a request toward them and creates no object |

## 8. Number hygiene

0094 (`feat/adr-0094-accumulating-seed`) and 0095 (`docs/position-is-not-a-share`) are resident
on their branches at the time of writing; 0096 is the next free number in `main`'s index and this
file claims it. A concurrent claimant renumbers the later writer; the index is the only place the
mapping is written down.

## 9. Implementation record

* **2026-09-10** — ADR written against misakas `e65ccf20` and MISAKA-Studio `7096533`. Branch
  `feat/adr-0096-everyday-lane` opened in both repositories. Step 1 of §6 begins on the gateway
  (Decisions 1–5's node half) and on the Studio (Decisions 1, 3, 4, 5, 11, 12); what has landed
  is appended here as it lands.
* **2026-09-10, Decision 8's fence declared** (`feat/adr-0096-partb-fence`, `b62e89c2`):
  `Params::palw_fp_decode_constraint`, `None` on every preset, Some-only in both fingerprints and
  in the schedule id (a shipped network is byte-identical to a build without the field), `never()`
  collapses to absence, and `validate_palw_v2` refuses a scheduled height in the sentence
  ADR-0082 Decision 11's refusal uses — the three halves are absent on this build. The wire says
  which side of it a node is on: `GetPalwProducerFactsResponse` version 7 adds
  `fp_decode_constraint_armed` (borsh suffix, fail-closed on an older writer; gRPC field 29; the
  service answers it at the candidate's score; `misaka palw facts` prints it). Tests: dormant on
  every preset and visible the moment it is not; refused at assembly by name; the borsh and gRPC
  round trips carry it and a version-6 writer reads as dormant (15 + 4 + 1 green). Nothing of
  Decisions 6, 7 or 9 exists yet: no v6 job, no automaton, no v3 refutation, no `render_answer_v2`.
* **2026-09-10, Decision 13's door and Decision 10's node half** (`0de50b0f`):
  `.github/ISSUE_TEMPLATE/model-request.yml`, `docs/model-requests.md` (the eight hands a request
  passes through, each with its ADR), `docs/components-manifest.md` (schema `misaka/components/v1`;
  a row points at the platform archive the release publishes and names the member — a loose
  binary URL would point at nothing), `scripts/misaka-components-manifest.py` (write / `--check` /
  `--validate` / `--self-test`, 39 checks) and one deploy step per platform writing
  `components-<platform>.json` beside the archive for the two components the job builds today
  (`kaspad`, `misaka`); the workers, gateway and rail become rows the day the job builds them,
  and the writer refuses any other name rather than guess a kind.
* **2026-09-10, Part A's node half** (`339ed0ad`, wired to the fence in `cc8b47f7`'s merge and
  after): `misaka-palw-gateway/src/surface.rs` (the request read once; `admit_request` before the
  queue and before the worker), `wire.rs` (tools as Qwen's own Hermes-style text under the
  `-tools/v1` template ids; `parse_tool_calls` touching no committed byte), the new crate
  `misaka-palw-constraint` (Decision 7's JSON-Schema subset, RFC 8785 — whose Appendix-B vectors
  found two real defects on the way: Rust's `{:e}` does not round half-even on a shortest tie, and
  serde_json's default float parser is not correctly rounded, so the crate enables
  `float_roundtrip` — and `constraint_id` under `misaka-palw/constraint/v1`), `misaka.format`
  served advisory and said so, `require_committed_format` refused before the inference,
  `GET /v1/models`, and the conformance corpus `docs/openai-surface/v1/` (22 cases; directory
  digest `5cdf928859dccdc7e841b6f4f9b01dc75a1080f3705e515ef1793cf6d08d72b9`, which the Studio's
  mirror pins). `ChainFacts::fp_decode_constraint_armed` reads the node's producer facts (wire
  version 7) and nothing else. Two choices to know: the system line created to hold a tool block
  is "You are a helpful assistant." (Qwen2.5's vendor line names Alibaba; Qwen3's writes nothing),
  and JSON rendered into the prompt is RFC 8785 so the job id is not a function of the client's
  key order. Not done: the `json` derived kind (Decision 9), and a live-worker run on this host.
* **2026-09-10, the gateway admits a bond-less identity exactly when it never commits**
  (`8d3d671c`, `a273d617`): Decision 10's last bullet on the node side — `identity.json` may omit
  the bond and the key under `--answer-never-commit`, `/health` says `bond: null`; refused by name
  otherwise, before any inference.
* **2026-09-10, the Studio (MISAKA-Studio `feat/adr-0096-everyday-lane`)**: `6a7c5d1` + `a3d9975`
  (UI: the sampling notice, the format badge, the context and job lines, tool calls shown as the
  JSON the app must run; conversations through the runtime with `localStorage` as the cache and a
  one-time migration; Export/Import; the Lane settings; "Request a model" — and, measured on the
  way, the desktop window has no opener, filesystem or dialog capability, so a link is shown with
  a Copy button and an export goes to the clipboard), `aaac111` (runtime: Decision 1's surface
  read from one table, tools/tool_choice/response_format forwarded to a child engine and to the
  lane, `node.sampling_policy` with `misaka.sampling`, `require_committed_format` refused before
  anything is sent, the summary job and the continue legs with `misaka.jobs[]` and
  `misaka.context`, the record keeps `misaka`; conversations as one JSON file each under the data
  directory with import from the Studio's export, the window's cache, OpenAI's
  `conversations.json` along `current_node`, or a generic list; the model-request URL),
  `7a5a31f` (README), `bf98e7a` (invariant 7: `testdata/openai-surface/v1` mirrors this tree's
  corpus byte for byte and pins the same digest), `01dfc05` (Decisions 10 and 11: every backend's
  `RuntimeFingerprint` from what its constructor stored, the rebuild predicate over routing
  inputs and fingerprints with a 61-leaf × 5-kind table test, `GET /api/v1/settings/effective`;
  `components.rs` as the one search order with the four spellings reduced to wrappers and pinned
  by a source test, the `misaka/components/v1` reader mirrored rule for rule, `GET
  /api/v1/components`, `POST …/install` through the verifying download manager (archive members
  refused by name for now), `misaka-studiod --check` / `--strict`, and
  `contrib/components/testnet-11.json` as the offline copy pinned to the class table). Tests at
  that point: core 49, runtime 192, integration 10; clippy 0.
* **2026-09-10, Decision 9's `json` kind** (`a582ba20`): grammar `json/v1` (one JSON value under
  RFC 8259 strictly, refused by name and never repaired, emitted as RFC 8785 bytes through the
  constraint crate's canonicalizer — where the RFC's IEEE-754 number rendering lives; the derive
  crate spells no float) and transformer `json/canonical/v1` (identity over canonical bytes).
  Two facts the implementation settled: the kind id is **28**, not 8 — `TEXT` already holds 8 in
  ADR-0078 D9's candidate table and an id is never reused, and the acceptance rule reads
  `kind != 0` only, so the row is consensus-inert; and the transformer-id pin was re-pinned on
  purpose, since every byte of `derive/src` is in every transformer id (ADR-0078 D3) — the ids
  of every shipped kind move with that commit, before any network relies on them. The gateway
  derives the json kind for a request that set `response_format` and named no `derive`. The
  schema-bound grammar id `H(json/v1 ‖ constraint_id)` does not exist yet (Part B).
* **2026-09-10, Decision 7 as pure consensus functions** (`28adec54`; nothing armed):
  `consensus/core/src/palw_decode_constraint_v1.rs` — the automaton form (a pushdown automaton of
  sorted byte-range frames with a 16-bit seen mask, so object keys are any order and at most once;
  64 KiB, 65,535 nodes, depth 16, an epsilon-hop cap), `constraint_id_v1`, `admit`,
  `decode_token_select_v3` (every lane admitted → v2 on every row; `T_q = 0` → v1), and
  `check_tiled_decode_token_refutation_v3` appended to `palw_step_refute.rs` (no constraint →
  the v2 verdict byte for byte; the one-disclosure "not admitted" arm; the two-disclosure arm
  with a forbidden beating lane not a fault). One thing the text above did not say and the court
  needs: **the EOG ids**, beside the token-to-bytes table — without them it cannot try Decision
  7's stop rule, and either convicts an honest stop or leaves the stop position untriable. The
  compiler (`misaka-palw-constraint/src/compile.rs`) admits RFC 8785's canonical form with key
  order free; `pattern` is refused by name, `minimum`/`maximum` stay post-hoc, and a single
  string's `maxLength` is practically about 250 under the 64 KiB cap. §4's producer cost is
  measured and corrected above. Still absent: job version 6, `render_answer_v2`, the engines'
  mask, the served token table, and the assembly refusal's removal.
* **2026-09-10, Decision 10's engine, and the artifact it could not run** (misakas `70d56519`,
  `11b293a9`; MISAKA-Studio `2d9e5e8`, `af9c4da`). Measured before building it: **the a16 worker
  refuses the published dense artifact at boot** (`qwen25-1.5b-a16.palwart`, sha256
  `a8c4e53e…`: "this artifact declares no tokenizer"), so an engine on the family worker could not
  serve the file every Studio downloads. The node tree gains `bind_tokenizer_file_v1` and
  `palw-class bind-tokenizer`, which sets that one field and MEASURES each row's registered root
  before and after: the tiled-map rows (`graph-v2`, `graph-v3`, `graph-v5@512`) keep the inventory
  root `1a7457f1…`, the one-byte-map rows move with the digest. The output's sha256 is
  `3f8fc5066bafae28…` — the file the testnet-11 fleet runs for free prompts, reproduced from the
  public artifact and the public `tokenizer.json`. The gateway admits the identity `{}` under
  `--answer-never-commit` with `--anchor` and adopts its worker's class. The Studio's `misaka`
  engine now supervises that gateway over the family worker, speaks to it through the pool
  path's own request code, empties its answer-only outbox at each load, and — for an unbound
  artifact on a machine that still has the retired `misaka-palw-serve` — falls back to it by name
  rather than taking the chat away. Two defects the smoke run through `misaka-studiod` found and
  fixed: the sampling notice applied only to an engine named `gateway`, and the `misaka` merge
  replaced the Studio's `ignored_fields` with the gateway's empty list. The Studio's Settings show
  the effective values and a Components page holds every binary to the manifest (`af9c4da`).

  **One decision is the operator's and is not taken here: publish the bound artifact.** The
  Studio's class table pins the unbound file's sha256, so every new install downloads a file the
  family worker refuses and either binds it or falls back to a retired binary. Uploading
  `3f8fc5066bafae28…` to `Misakachain/Qwen2.5-1.5B-PALW-A16-runtime` and moving the pin is one
  upload and one line, after which the fallback has no user; it is an outward-facing act, so it
  waits for the operator.

* **2026-09-10, Part B's job, engine and seat halves** (branch `feat/adr-0096-partb-drill`:
  `231d75cb`, `318d7800`, `03f643a6`). B1–B4 and B7 as §10 says, plus B9–B12 below: the version-6
  job and payload, the automaton on every free-prompt material, the pinned Qwen2.5 token table
  (`01e9c31c…`, 151,936 ids, lowest EOG 151,643, reproduced by an independent implementation), the
  a16 engine's masked decode with the table passed in by its caller, the worker's lazily built table,
  the seat's masked replay (`--palw-class-tokenizer`), the rail's version-6 build and the
  entrance's masked and committed modes. The fence is still refused at assembly: the court half
  (B5, B6, B10) is being built beside this, and `--palw-fp-constraint-devnet` arms the fence on a
  private devnet only through the same check. The first constrained drill ran on a build whose
  refusal was bypassed locally for that run only (never committed), while the court half was
  written; its evidence is recorded when it finishes.

## 10. Amendment 2026-09-10 — what the court needs, found while building the drill

Decision 7 said the court "recomputes `s` by running the automaton over the rendered prefix —
which needs the class's token-to-bytes table (Decision 8)", and Decision 8 said the table is
"served material". Building toward the drill showed that this is not adjudicable as written. The
court is run by every node inside the fold, a node holds no tokenizer, and the only
tokenizer-derived value the chain can reach is the artifact's `tokenizer_commitment` — a flat hash
of `tokenizer.json`, from which no single token's bytes can be proven. And the claim record the
court reads (`PalwClaimStateV2`) holds roots, not the job. A court that needs a file only some
nodes have is a consensus split, and a court that cannot try a constrained token convicts an
honest one: a challenger who could state "no constraint" would win against every masked token.
So Decisions 7 and 8 are completed here, and the fence stays refused at assembly until every
item below is in the build.

**B1 — the job's version 6 layout.** `PalwFreePromptJobV3` serializes byte-for-byte as today for
version 5 and appends `constraint_id: Hash64` for version 6 only, so every version-5 job id, claim
id and pinned golden stays what it is. Version 6 requires a non-zero `constraint_id`, and version
5 has none, so one meaning never has two encodings. The worker result and the commitment payload
move with the job, as they already do (one version constant today).

**B2 — the payload carries the constraint.** Under `PublicDa` a version-6 payload appends the
constraint's canonical bytes, bounded to **16 KiB** (not the automaton's own 64 KiB — the bytes
must fit a court carrier beside the rest of a close, B5). Acceptance parses them canonically and
requires `constraint_id_v1(bytes) == job.constraint_id`. Seats replay from it. `PanelDa` with
version 6 is refused by name until a private route for the constraint exists.

**B3 — the answer is committed token by token.** For a version-6 job, the family's rendered-output
hash is `rendered_segments_hash_v1` over the per-token byte strings, length-prefixed, in order —
not a keyed hash of the ids. `output_root = output_commitment_v2(context, ids, rendered)` then
binds each position's bytes, so a close can carry the rendering and the court checks it against
the claim's own `output_root` without a new field anywhere.

**B4 — the token tables are the build's.** For each tokenizer a class may constrain under,
consensus-core pins `(tokenizer_commitment, vocab_len, token_table_root)`, where the root is a
Merkle root over leaves `H(domain ‖ id ‖ len ‖ bytes)` for every id in `0..vocab_len` (an id the
tokenizer cannot render has the empty-bytes leaf and is never admitted). The pin is produced by a
script from the tokenizer file and checked against the file by a test, the Gumbel table's shape
(ADR-0082 Decision 11). A tokenizer with no row cannot carry a version-6 job. This is ADR-0067's
rule applied to tokenizers: classes are chain data, kernels — and now token tables — are the build.

**B5 — the constrained decode close.** A new close arm carries the tiled pin (as today), the
version-6 job (checked: `fp_job_id_v3(job) == binding.job_context.job_id`, and the binding is
already checked against the claim's `execution_root`), the constraint bytes (checked against
`job.constraint_id`), and the per-token segments (checked: `output_commitment_v2` over the pin's
ids and `rendered_segments_hash_v1(segments)` equals the claim's `output_root`). The court walks
the automaton over `segments[..p]`. The one-disclosure arm needs no tile: the committed token at
`p` is not admitted from `s_p` (under B7's finish rule). The two-disclosure arm adds the beating
lane's bytes with a Merkle opening against the pinned table root, and convicts only if that lane is
admitted and its key beats the committed one's.

**B6 — the rendering close.** A lie about the segments themselves is tried separately. The close
carries the segments (checked against `output_root` as in B5), a position `q`, and the table
opening for `ids[q]`, and convicts if `segments[q]` is not the table's bytes. A challenger's own
segmentation cannot be substituted, because the segments are checked against the claimant's
`output_root`, not supplied on trust.

**B7 — the finish rule.** Once no lane is admitted (a complete root value admits nothing), the
committed token is the lowest EOG id at that position and at every later one; the automaton is in
a distinguished finished state, and the run still ends at the declared budget
(`ExactBudgetReached`), so the job context and its hash are built before the run exactly as today.
`render_answer_v2` renders up to the first committed EOG id, so the answer a derivation reads is
exactly the constrained value.

**B8 — the qwen36 family.** Its engine does not mask yet; a version-6 job for it is refused by the
worker by name.

**What the drill proves, and what it does not.** On a private devnet with the fence scheduled: a
constrained claim is committed, accepted, replayed by every seat and reaches `Final`. The court
arms (B5, B6) are proven by tests through `adjudicate_court_close_v2`, the fold's own entry, over
a real constrained run on a fixture-sized artifact: an honest run acquitted, an altered id
convicted by the one-disclosure arm, a forbidden beating lane not a fault, a lying segment
convicted by B6. A live court round trip is the court drill's job, not this one's.

**B9 — no interval arm for a version-6 claim.** The interval replay (ADR-0077 Decision 8, ADR-0082
Decision 9) teacher-forces the committed ids and re-selects each token by the plain argmax, which
a masked decode is not, so it would disagree with every honest constrained claim. A seat skips it
for version 6 and replays the whole job masked. The a16 capture is over the transport cap at
every useful length (37 MB for 60 tokens, measured), so the seat reads the job, the prompt and the
automaton from the answer envelope, which therefore carries the automaton for version 6 (as do
`FPM1` and `FPC1`; B2's "seats replay from it" is the material, bound to `constraint_id` by every
decoder). A seat without the pinned table answers `Incapable` — it cannot judge — never
`Unavailable` against an executor that served everything.

**B10 — past the fence, a free-prompt decode close carries its job.** Neither the claim record nor
`PalwJobContextV2` says whether a claim was constrained: the context holds no version and no
constraint id. So the unconstrained decode arms (`DecodeToken`, `DecodeTokenTiled`) cannot tell a
masked token from an unmasked one, and a challenger who used them against a version-6 claim would
convict an honest masked token whenever the unconstrained argmax differs. On a network that armed
the fence they are refused by name for a free-prompt claim; the job-carrying arm (B5) is the only
decode close for one, running the v2 check for a version-5 job and the v3 check for a version-6
job. Attempt-lane claims keep the old arms. The arithmetic arms are unaffected: they authenticate
the committed ids against the claim's logits trace root and recompute arithmetic over them, and
never re-select a token. Measured while writing this: no tool in the tree assembles a decode-token
close today, for either version — the panel's automated court builds arithmetic closes only, and
the CLI builds none — so a selection fault is not prosecuted automatically on any network. For a
version-6 claim the seat's masked replay is what catches one: a different committed token moves the
execution root, and the claim gathers no licence. Building the job-carrying close in a
challenger's tool is the court's remaining work, not the fence's.

**B11 — the pin carries the lowest end-of-generation id.** B7's finish rule commits the class's
lowest EOG id, and the court tries it from the automaton and that id alone, so the id is the
build's, beside the root (151,643, `<|endoftext|>`, for the Qwen2.5 row; checked against the
tokenizer file). Byte-completeness — every one of the 256 bytes is some id's whole rendering, which
is what lets "no lane is admitted" be read as "no byte continues" — is certified by
`palw-token-table` and the pin's test. Acceptance refuses a version-6 job whose tokenizer has no
pinned table (`ConstraintTokenizerUnpinned`), because no court could try a token of it.

**B12 — what the entrance learned from the first masked answers** (Qwen2.5-1.5B-Instruct A16,
answer-only gateway, 2026-09-10).

* **One space after each `:` and `,`.** The compiler admitted RFC 8785's canonical form only, and a
  model whose next token is a space-quote was forced onto the best token that is not: "a JSON object
  describing a cat" came back `{"name":null,"age":null}`. The automaton now admits exactly one space
  (0x20) after each separator, inside `enum`/`const` literals too, and nothing else — no newline, no
  second space, nothing around the root — so it cannot loop and B7's finish still fires at the root
  value's last byte. The same prompt answers `{"name": "Whiskers", "age": 3}`. The derivation strips
  it (Decision 9 canonicalizes), as it canonicalizes a number.
* **The model reads the schema in the client's member order.** The instruction rendered the schema
  canonically, which sorts members, and a light model answers in the order it is shown: for
  `{label, confidence}` it wrote `"confidence":0.5` first and then `"label":"neutral"` for "the
  battery died after two days"; shown in the client's order it wrote `"label":"negative"` and 0.95.
  The gateway reads the order from the request body (the workspace's `Value` sorts, and must keep
  doing so) and renders RFC 8785 scalars in that order; the constraint id, the automaton and the
  report stay on the canonical bytes, so two orders of one schema are one constraint. OpenAI emits
  members in schema order too. An entrance in front of the gateway must forward `response_format`
  verbatim for this to reach the model (the Studio does not yet).
* **`finish_reason` is `stop` when the answer ended itself.** A masked answer's EOG renders empty,
  so the byte comparison that decided `stop` saw no cut and said `length`; the stream records that
  the answer ended.
