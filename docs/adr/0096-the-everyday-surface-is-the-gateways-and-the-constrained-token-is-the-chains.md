# ADR-0096 — The everyday surface is the gateway's, and the constrained token is the chain's

* Status: PROPOSED 2026-09-10
* Builds on: [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) (the
  free-prompt lane, the gateway, `PalwFreePromptJobV3`), [0078](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md)
  (derived artifacts, the kind table, the consumer's read path),
  [0082](0082-the-lane-the-court-can-try-is-the-lane-a-person-would-use.md) Decisions 10 and 11
  (the decode numerator and the seeded argmax, and their shared fence
  `Params::palw_fp_decode_rules`), [0049](0049-the-court-tries-one-step-and-the-step-is-a-leaf.md)
  Decision E (the tiled logits pin and its two-disclosure refutation).
* Amends nothing. Every rule here is either **outside consensus entirely** (Part A) or **behind a
  new dormant fence** (Part B). No shipped preset's fingerprint moves.

## 0. The sentence this ADR is

A person with an OpenAI-shaped client should be able to point it at a MISAKA gateway and have it
work — parts-shaped content, tools, a model list, a refusal that names the rule — and **none of
that is a consensus change**, because the chain hashes the rendered prompt bytes and does not care
which JSON shape produced them. The one thing an everyday client asks for that the chain *must*
own is **`response_format`**: a schema that binds what token may be committed is a change to the
decode rule, it goes behind its own fence, and until an engine and a transition carry it the fence
is refused at assembly rather than armed into a lane that burns inferences.

## 1. What is actually there today, measured

Not asserted — read off `main` at `7f4dded`, on 2026-09-10. This section exists because the
previous pass over this design carried two claims that did not survive being checked.

**The gateway's surface is three routes.** `misaka-palw-gateway/src/main.rs:1563` answers anything
else with the list: `POST /v1/chat/completions`, `GET /health`, `GET /v1/artifacts/<derived-id>`.
There is no `/v1/models`. A client that enumerates models before its first call — which most of
them do — fails before it ever sends a prompt.

**A message's content is one `String`.** `ChatMessage { role: String, content: String }`
(`main.rs:560`). The OpenAI content-parts form — `content: [{"type":"text","text":"…"}]` — is a
parse error, and the client is told `request body is not a chat completion: …` with serde's
message, which names a line and column in a body the user did not write by hand. There is no
`tools`, no `tool_choice`, and the `tool` role has no rendering.

**`max_tokens` works, and 256 is a default rather than a wall.** `main.rs:873` reads
`chat.max_tokens.unwrap_or(config.max_decode_default).clamp(1, config.max_decode_cap)`, with
`max_decode_default = 256`, `max_decode_cap = 1024` (`main.rs:1139–1140`) and
`HARD_MAX_DECODE_CAP = 4_096` (`main.rs:114`). **A note in the previous pass said the 256 was a
wall lifted by commit `0001f34c`; both halves are wrong.** `0001f34c` (2026-09-05) chunks the
free-prompt lane's *retained-trace manifest* at 256 and has nothing to do with decode length, and
the 256 here was never a ceiling — it is what a caller gets for not asking. The real ceilings are
the operator's `--max-decode-cap` and the hard 4,096 above it. Nothing in this ADR changes them.

**The sampling fence is already the right shape, and this ADR copies it.**
`sampling_from_request` (`main.rs:630`) refuses a `temperature` or a `seed` **before the inference**
whenever `ChainFacts::fp_decode_rules_armed` is false — by the fence's name, with the reason that
the transition would answer `SamplingNotArmed` after the work was already paid for. That is the
pattern Part B's `response_format` follows exactly, and Decision 6 says why the *advisory* case
differs from it.

**`misaka-palw-serve` is gone, and a Studio that shells out to it is calling a binary this tree
does not build.** Retired 2026-09-02 in `2f688bc` — "one runtime — `v3-serve` on both family
workers, `misaka-palw-serve` retired" (ADR-0077 Decisions 1/2/6). Any surface still naming it is
naming a hole.

**The kind table has 27 entries and its next id is 28.** `palw_derived_v1.rs`'s `kind::ALL` runs
`SCENE = 1` through `PROCEDURAL = 27`, and `kind_table_ids_and_names_are_unique_and_never_zero`
pins `kind::name(28) == None`. Decision 9 takes 28.

**The producer-facts wire is at version 6.** `GetPalwProducerFactsResponse` stores `store!(u16, &6)`
(`rpc/core/src/model/message.rs:2378`); version 4 added `fp_decode_rules_armed`, version 6 added
`panel_da_armed` and `prompt_ids_merkle`. Every version is a strict suffix read fail-closed.
Decision 7's flag is version 7.

## 2. The line this ADR draws, and why it is drawable at all

The free-prompt job commits **rendered prompt bytes** — `PalwFreePromptJobV3` carries the prompt's
token ids and their hash, not the JSON a client posted. So there are exactly two kinds of change an
everyday client's request can ask for:

* **A change to what bytes get rendered.** Content parts, a tool block, a system message the client
  synthesises. The chain sees a prompt, hashes it, and is unable in principle to tell which JSON
  shape produced it. **Consensus-inert by construction, not by care.** This is Part A.
* **A change to which token may be COMMITTED at a position.** `response_format: json_schema` is
  this and only this: it is a claim that the decode rule refuses a token the schema forbids. The
  committed token is the thing the court tries. **A consensus rule.** This is Part B.

The line is worth stating because the two halves have opposite risk profiles and were, in the
previous pass, at risk of being shipped as one change. Part A can land on any network at any time
and cannot move a fingerprint. Part B cannot land at all until an engine implements it, and this
ADR's Decision 7 makes the build *say so* rather than let an operator arm it.

## 3. Decisions

### 3.1 Decision 1 — the everyday surface is the gateway's, and the prompt bytes are the chain's

The gateway grows the OpenAI shapes an ordinary client sends. It renders them, by the class's own
chat template, into the same prompt bytes `PalwFreePromptJobV3` already hashes. No consensus
artifact gains a field, no fingerprint moves, and every rule in Part A is testable by rendering a
request and comparing bytes.

The corollary is the constraint on Part A, and it is the whole discipline: **if a Part A feature
cannot be expressed as "these bytes instead of those bytes", it is not a Part A feature.** Tools are
(Decision 3). `response_format` is not (Decision 6).

### 3.2 Decision 2 — a message's content may be parts, and the rendering is stated

`content` accepts a string or an array of parts. A text part contributes its `text`. A part of any
other type — `image_url`, `input_audio`, anything a client invents — is **refused by name**, naming
the type and saying this class is text-in/text-out; it is never dropped silently, because a client
that sent an image and got an answer about the text beside it has been told a false thing about
what ran.

Concatenation is by `""` with no separator inserted: a client that split one sentence across two
text parts gets one sentence, which is the only rendering that makes the parts form and the string
form agree on bytes. `MAX_CHAT_MESSAGES = 64` and `HARD_MAX_PROMPT_BYTES = 64 KiB` are unchanged and
are checked **after** flattening, so the parts form cannot be used to get around either.

### 3.3 Decision 3 — tools are prompt bytes, declared and answered in the class's own format

`tools` and `tool_choice` are accepted and rendered into the prompt in the class's chat-template
format — for the Qwen-family classes this tree serves, the `<tools>` block and the
`<tool_call>` / `<tool_response>` turns. The `tool` role renders as the class's tool-response turn,
and `assistant` messages carrying `tool_calls` render as the class's tool-call turn.

Three rules that follow from Decision 1 and are not negotiable:

* **The gateway never executes a tool.** It renders the declaration and it renders the reply the
  client supplies. A gateway that called out to a tool would be putting a side effect inside a
  claim, and the claim is supposed to be a pure function of prompt bytes.
* **Parsing the model's tool call out of the answer is the client's, and the gateway's rendering of
  it is advisory.** The gateway may surface a parsed `tool_calls` array for convenience; the
  answer's *bytes* remain the product, and `output_root` commits those bytes and not the parse.
* **`tool_choice: "required"` is refused below Part B's fence.** "Required" is a decode constraint
  wearing a different name — it is the claim that a non-tool-call token cannot be committed — and
  Decision 6's rule applies to it unchanged. `"auto"` and `"none"` are renderings and are fine.

### 3.4 Decision 4 — `/v1/models` answers what this gateway can actually run

`GET /v1/models` returns the class this gateway is bound by its `identity.json`, and nothing else.
Not a catalogue of the registry, not every class the chain knows: the list a client uses to pick a
model must be the list of things a request to *this* endpoint will actually be served by.

The row carries the OpenAI fields a client indexes on (`id`, `object`, `created`, `owned_by`) and a
`misaka` object with the facts a MISAKA user needs and cannot get elsewhere: the class id, the
artifact root, whether the chain certifies the class (`fp_certified`), the effective decode cap, and
the two fence states (`fp_decode_rules_armed`, and Decision 7's `fp_decode_constraint_armed`). A
client that never reads the `misaka` object still works; one that does never has to guess.

### 3.5 Decision 5 — a refusal names the rule it comes from

Every 4xx this surface produces names the thing that refused: the fence, the cap, the part type,
the message count. The existing sampling refusal (`main.rs:650`) is the model — it names
`Params::palw_fp_decode_rules`, says what the transition would have answered, and says what to send
instead.

This is a decision and not a style note because the alternative has a cost the user pays: a generic
`400 Bad Request` on a network whose fence is dormant is indistinguishable from a malformed body,
and the user's next move is to retry the same request.

### 3.6 Decision 6 — `response_format` is ADVISORY below the fence, and `misaka.format` says so

`response_format` is accepted below Part B's fence and is **rendered into the prompt** — the schema
becomes an instruction the model is asked to follow — and the answer is returned with a `misaka`
object saying exactly what happened:

```json
"misaka": { "format": { "requested": "json_schema", "enforced": false,
                        "reason": "palw_fp_decode_constraint is dormant on this network",
                        "valid": true } }
```

`enforced` is whether the decode rule bound the tokens. `valid` is whether the answer that came out
parses and validates against the schema — checked by the gateway, after the fact, as a report and
never as a retry.

**Why advisory here and refused for `temperature`.** A temperature the chain will not admit makes
the *commitment* unsubmittable: the inference is spent and the claim is refused, so the honest
answer is a 4xx before the model loads. A schema the chain does not enforce makes the commitment no
less valid — the job is an ordinary greedy job over prompt bytes that happen to contain a schema —
so refusing it would deny a working request in the name of a rule that has no opinion about it. The
difference is not taste: it is whether the dormant fence changes what the transition does with the
result. `tool_choice: "required"` is on the temperature side of that line, which is why Decision 3
refuses it.

**Past the fence, `enforced` is true and the refusals become the transition's.** The same field, the
same object, and the client's code does not change.

### 3.7 Decision 7 — a decode constraint is a consensus rule and gets its own fence

**`Params::palw_fp_decode_constraint`.** A bare fence, top level, `None` on every shipped preset,
named in `consensus_identity_id` and in the schedule id, and mapped by `fork_id_v1` — the
`palw_fp_decode_rules` shape exactly, for its reasons.

Past it, `PalwFreePromptJobV3` carries a constraint — a canonical schema document and its hash —
and the committed token at each position is the seeded argmax **over the admitted lanes only**:

```text
committed_j = argmax over { j : admitted(state_p, j) } of  decode_lane_key_v2(values[j], seed, p, T_q)
```

`state_p` is the constraint automaton's state after the committed prefix, which both sides derive
from material the claim already commits. Admission is a predicate on `(state_p, j)` and nothing
else.

**It is a SEPARATE fence from `palw_fp_decode_rules`, and that is deliberate.** ADR-0082 states why
its own two decisions ride one fence — a numerator without a sampler earns nothing, a sampler
without a numerator repeats — and neither argument reaches here. A constraint is well-defined at
`T_q = 0`: constrained greedy is the most useful configuration this lane has, and it is exactly what
`response_format` means to a client that never sets a temperature. Folding the constraint into
`palw_fp_decode_rules` would make structured output unreachable on any network that wants it without
also arming a sampler, and would make the sampler's flag day carry two rules a court has to try.

**The empty constraint is byte-identical to today.** When a job declares none, `admitted` is `true`
for every lane, the argmax is over the whole row, and the rule is `decode_token_select_v2` unchanged
— the same property ADR-0082 relies on for `T_q = 0`, and the same kind of test pins it.

### 3.8 Decision 8 — the constraint is a per-lane predicate, so the court is unchanged

This is the reason the design is carriable at all, and it is the only property Part B may not give
up.

ADR-0049 Decision E's refutation is two disclosures: open the committed lane's tile and a beating
lane's tile, recompute both keys, compare. ADR-0082 Decision 11 preserved that by making the Gumbel
term a *per-lane* function — `decode_lane_key_v2` reads one value and never another lane's.
Decision 7 preserves it the same way: `admitted(state_p, j)` reads the automaton state and the lane
index, and no other lane's logit.

So a refutation past the fence is the same two disclosures plus one term both sides can already
compute:

* the challenger opens the committed lane's tile and the lane it says should have won;
* both sides derive `state_p` from the committed prefix, which the claim commits;
* a claim is refuted if the beating lane is admitted and beats the committed key, **or** if the
  committed lane is itself not admitted.

The second arm is new and is the whole enforcement: a class that ignores the constraint commits a
forbidden token, and one disclosure convicts it. **No new disclosure class, no whole-row opening.**
A constraint that needed the full row — a renormalising one, or one that scores a lane against the
row's mass — would be the 993 KB row `palw_close_budget` already refuses, and is out of scope for
the same reason a softmax sampler was.

The canonical form is RFC 8785 (JCS) over a stated JSON Schema subset, so that two nodes deriving
an automaton from "the same schema" derive the same automaton. The subset, its canonicalisation and
the automaton are a pure function with its own tests, in their own crate, so the court and the
engine link the same code.

### 3.9 Decision 9 — `json` is a derived kind, id 28

ADR-0078's kind table gains `JSON = 28`, `"json"`. A structured answer is a thing a consumer
verifies by recomputing `output_root`, `dsl_hash` and `artifact_hash` — invariant X6 — and it had no
kind to be filed under; `text` is the answer's own bytes and `data` is ADR-0078's tabular kind.

The table is append-only and the pins move with it: `kind::name(28)` becomes `Some("json")` and the
"never assigned" probe moves to 29. Nothing about ids 1–27 changes.

### 3.10 Decision 10 — a runtime reports its EFFECTIVE settings, and their fingerprint

A surface that reads a config file, a set of flags and a set of environment variables, and then acts
on the merge, must be able to say what the merge WAS. The reported object carries every effective
value and a fingerprint over it, and the fingerprint is what a bug report quotes.

The rule that makes it worth having: **the reported object is produced by the same function the
runtime acts on**, never by a second walk over the sources. A reporter that re-derives is a reporter
that can disagree with the thing it reports on, and the disagreement shows up exactly when someone
is trying to explain a misbehaviour.

### 3.11 Decision 11 — one components manifest, one resolution rule

Where a runtime locates binaries, models and artifacts, there is ONE manifest and ONE documented
order in which a candidate wins: explicit flag, then configured path, then the manifest's entry,
then the platform default — first hit wins, and the effective answer is in Decision 10's object with
the reason it won.

The failure this replaces is a search order that lives in three functions and disagrees with itself
about which of them looked first — which is how a surface ends up calling `misaka-palw-serve`
(§1) on a tree that stopped building it eight days ago.

## 4. Security — the four principles, checked

**Nothing here lets an unfunded party spend a funded party's resources.** Part A's new shapes are
parsed inside the existing body cap (`MAX_REQUEST_BODY_BYTES`, 1 MiB), flattened before the message
and prompt caps are applied (Decision 2), and rate-limited by the per-source admission that already
guards the route. `tools` and `response_format` enlarge the *prompt*, which is bounded by
`HARD_MAX_PROMPT_BYTES`, and cost the requester their own budget.

**Nothing here makes a claim cheaper to forge.** Part A does not touch what is committed. Part B
makes admission *narrower*: a constrained job's committed token must be admitted, so a class has
strictly fewer tokens it can commit and get away with, and Decision 8's second refutation arm is a
new way to be caught rather than a new way to escape.

**Nothing here is armed by existing.** The constraint crate, the automaton and Decision 7's field
all exist dormant. `palw_fp_decode_constraint` is `None` on every preset, and — the operative part —
**arming it is REFUSED by `validate_palw_v2` on this build**, for the reason ADR-0082's own refusal
gives: no engine implements constrained decode, the `validate_*_v3` entry points would pass the flag
as a literal `false`, and the only reachable effect of arming it would be a lane that spends
inferences on commitments the transition then refuses. The refusal lifts on the build that carries
both halves. `never()` is absence and is exempt.

**Nothing here widens what a court must hold.** Decision 8 is the check, and it is stated as a
constraint on the design rather than as a property of it: a constraint form that cannot be decided
per-lane does not go in.

One residual, named rather than argued away: **a schema is prompt content below the fence and a
rule above it, and the same request means different things on the two sides.** Decision 6's
`misaka.format` object is the mitigation — `enforced` is on the wire in both states, so a client
that cares can tell, and one that does not still gets an answer.

## 5. Invariants the tests must hold

1. **The empty constraint is the shipped rule.** For random rows, seeds, positions and
   temperatures, selection under a constraint that admits every lane equals
   `decode_token_select_v2` exactly. (`the_empty_constraint_is_the_shipped_rule_on_every_row`)
2. **The committed token is admitted.** For any non-empty constraint, the selected lane satisfies
   `admitted(state_p, ·)`, and if no lane is admitted the rule answers a refusal rather than a lane.
3. **Canonicalisation is a function.** Two schema documents that differ only in key order and
   insignificant whitespace produce one canonical form and one hash; a document outside the subset
   is refused by name at the boundary and never silently narrowed.
4. **Admission reads one lane.** A property test: perturbing any lane other than `j` never changes
   `admitted(state_p, j)`. This is Decision 8 as an executable statement.
5. **The fence is dormant and unarmable.** Every shipped preset leaves `palw_fp_decode_constraint`
   `None`; `palw_fp_decode_constraint_active_at(u64::MAX)` is false on all of them; a preset with it
   armed fails `validate_palw_v2` by name; `Some(never())` normalises to `None`.
6. **The fingerprint does not move.** Every shipped preset's `consensus_params_id` and
   `consensus_identity_id` are byte-identical to the build without the field — pinned by the
   existing fingerprint pins, which must not need editing for this change.
7. **The wire is a strict suffix.** A version-7 `GetPalwProducerFactsResponse` round-trips
   `fp_decode_constraint_armed`; a version-6 peer reads it as `false`; the flag survives the gRPC
   conversion in both directions.
8. **Part A renders to bytes.** For each new request shape, the rendered prompt equals the rendered
   prompt of the equivalent string-form request, byte for byte — the corpus is checked in, and both
   the gateway and any other runtime that claims this surface are checked against the same files.
9. **The kind table stays append-only.** Ids 1–27 keep their names, `kind::name(28) == Some("json")`,
   `kind::name(29) == None`.

## 6. Order of work

1. **The fence's scaffolding** — Decision 7's field, its accessors, the presets, the identity and
   schedule hashes, `fork_id_v1`, the arming refusal, the RPC flag at version 7, the CLI display,
   and invariants 5–7. Consensus-inert and independently landable; **this is what the branch
   `claude/adr-0096-partb-fence-uwo64p` carries.**
2. **The constraint crate** — the schema subset, RFC 8785 canonicalisation, the automaton, and
   invariants 1–4. A pure function with no consensus wiring.
3. **Part A in the gateway** — Decisions 2–6 and invariant 8, with the corpus.
4. **Decision 9's kind**, with invariant 9.
5. **Decisions 10 and 11** in the runtimes that have the problem.
6. **The engine's constrained decode**, and only then the build that may lift Decision 7's refusal.

Steps 1–5 can land on any network without a flag day. Step 6 is a flag day and is not scheduled by
this ADR.

## 7. What is deliberately not decided

* **When `palw_fp_decode_constraint` is armed anywhere.** It is not scheduled on testnet-11 and no
  card states it. Arming it is an operator decision on a build that does not exist yet.
* **Which schema subset, exactly.** Decision 8 fixes the *shape* of the answer — a canonical form, a
  per-lane automaton, one crate — and step 2 fixes the subset with its tests. Writing the subset into
  this ADR would freeze it before it has been measured against a real tokenizer's vocabulary.
* **Images and audio.** Decision 2 refuses non-text parts by name. A multimodal class is a class
  question, not a surface question.
* **Tool execution.** Decision 3 says the gateway never executes; whether some other component may
  is out of scope.

## 8. Number hygiene

`docs/adr/README.md` recorded **0096 as the next free number** before this file; this ADR takes it
and the README's marker moves to 0097 in the same commit. If a concurrent session claimed 0096
first, this file renumbers — it is the later writer by the README's rule, and nothing outside
`docs/adr/` cites it by number yet except the fence's doc comments, which move with it.
