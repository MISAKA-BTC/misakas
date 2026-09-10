# ADR-0097 — A model's fit is a lookup, and the entrance says its limits before the first token

* Status: PROPOSED 2026-09-10; **IMPLEMENTED the same day, consensus-inert** (§9). No fence, no
  field of `Params`, no object, no fingerprint moves: Decision 1 is a report over predicates the
  chain already runs, Decision 2 is two JSON objects on the gateway, and Decisions 3–5 are what
  the report says about the question that prompted it.
* Builds on: [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) (Decisions 1–4:
  the ladder is minted once, the window binds, the arity is derived; §5: every number is a
  generated artifact), [0082](0082-the-close-is-flat-in-the-context.md) (Z4: the window gate;
  Decision 3: the arity; Decision 9: the seat's window is the drill's),
  [0081](0081-long-context-the-input-is-a-state-chain.md) (§1.1: the hybrid is the structurally
  better long-context candidate; §1.2: the ids are the linear term; Decision 3: the tiled root),
  [0080](0080-the-answer-is-long-the-verified-unit-is-short.md) (§1: the residue is the width),
  [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) (Decisions 12–14: the
  ladder, the rows, the canonical job; Decision 16: `PanelDa`),
  [0096](0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md)
  (Decision 1: the surface; Decision 5: a long thread is a chain of jobs; Decision 8: the token
  table as served material; Decision 10: the manifest), [0084](0084-the-ids-ride-the-capture-stays-home.md) /
  [0086](0086-the-opening-carries-the-fold-not-the-leaves.md) (what a seat is served),
  [0093](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) (the
  responder), [0079](0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md)
  (the host).
* Amends: nothing. Supersedes nothing.

## 0. The sentence this ADR is

**The chain can carry a model the size of Kimi K3 at ten positions of context, and nothing wider,
and the reason is nine walls with names — four of them numbers inside the ruleset id.** A 2M
context is refused for every model deeper than eight layers by the first of them, before any
court is consulted. This ADR does not move a wall. It makes the question "can this chain carry
model X at context C" a table generated from the chain's own predicates (Decision 1), makes the
entrance publish the limits a client needs before its first request (Decision 2), and names, wall
by wall, what a network that wants a wider model must mint with (Decision 5) — so the next person
to ask arrives at a lookup rather than at a conversation.

## 1. What was measured

The operator's question (2026-09-10):

> 現在のMISAKA chainはkimi k3くらいのdenceモデルを受け入れる基盤として 2M以上のコンテキストは
> […60 項目のチェックリスト…] などモデルを実用的に使用する基盤ができているか またできていなく
> 不十分ならADRを記述して実装を行なって

Read on `feat/adr-0096-everyday-lane` at `70d56519` (misakas) on 2026-09-10. Every figure below is
what `misaka-palw-base0 --bin palw-model-fit --preset rc` printed on that tree, from the
predicates named beside it; the binary is the authority and this section is its record
(ADR-0092 §5). The pinned figures are pinned as **limitations** by
`consensus/core/tests/palw_adr0097_model_fit.rs`, so that the day one moves, a test says so.

### 1.1 The walls, and where each ceiling lives

A class meets these in this order, and `verify_class_admission_v5` refuses at the first one it
meets. Read at the point of judgement "every scheduled fence armed" (`palw_kary_court` armed,
`palw_context_ladder` dormant, prompt ids `Flat` on testnet-11).

| wall | the predicate | the ceiling lives in | RC value |
|---|---|---|---|
| geometry ceiling | `n_ctx × layer_count ≤ PALW_STEP_MAX_ENUMERATION` | `PalwShapeProfileV3::validate_geometry` — a code constant that gates every `ClassRegistered`, so moving it is a ruleset move | 16,777,216 |
| ladder | the whole context as prefill, in leaves, ≤ `max_step_leaf_count` | `PalwCourtParamsV2`, inside `palw_ruleset_id_v2` (ADR-0092 D4) | 2^26 |
| close bytes / chunks / terminal macs / operand count | the widest close ≤ the court's ceilings | `PalwCourtParamsV2`, the same id | 2,250,000 B / 27 carriers / 2^24 / 8 |
| court window | `moves × turn_deadline + reserve < window_court` at the arity the court plays | the lattice windows, the same id (ADR-0082 Z4) | 3,000 DAA, 42 DAA a turn, 2 terminal |
| state chunks | the attention cache at `n_ctx`, tiled at 16, ≤ `PALW_STEP_LEG_MAX_STATE_CHUNKS` | `palw_step_leg`, the checkpoint leg's cap | 65,536 |
| public-da payload | the prompt ids on a `PublicDa` commitment ≤ one standard transaction | `PALW_STANDARD_TX_BYTES`; under `PanelDa` the ids do not ride (ADR-0077 D16) | 120,000 B |

Two quantities are reported and are not walls: what one JOB may answer (`min(max_decode_tokens,
PALW_V2_MAX_TRACE_EVENTS)` = 1,024 on both presets — a bound on an answer, which ADR-0096 Decision
5 chains, never on a class), and what a SEAT must hold (§1.5 — a host fact no ruleset states).

**The close past the ladder is not a number.** The court-cost derivation's walk is capped at the
ladder (audit D H-5), so a row the ladder refuses has an *unpriced* close, not an admitted one.
The generator prints `unpriced` there and never `admitted`; the widest context the four close
walls admit can therefore never exceed the ladder's.

### 1.2 The rows the chain carries, and the widest context each wall admits them at

Both shipped presets register one row with a carried profile: the dense graph-v5 row at 512
(`4277d84f…`). On the RC it is admitted on every wall — and **at the wall** on the window: 65 moves
× 42 DAA + 216 reserve = 2,946 against 2,999, ADR-0092 §8's finding restated as a lookup. The
hybrid family's row at 8 is admitted on the RC (close 160,100 bytes = 2 carriers of 27) and
**refused on the devnet by the close alone** (its ceiling is one carrier, 83,333 bytes): the devnet
cannot carry the hybrid tier, and nothing in `docs/` said so until this table.

| candidate (RC) | geometry | ladder | close | window | chunks | public-da | **fit** |
|---|---|---|---|---|---|---|---|
| Qwen2.5-1.5B A16 graph-v5 (28 layers) | 599,186 | **651** | ≤ ladder | **512** | 18,720 | 30,000 | **512** |
| Qwen3.6-35B-A3B graph-v5 (40 layers) | 419,430 | **204** | ≤ ladder | 512 | 52,416 | 30,000 | **204** |
| Kimi K3 stand-in (92 layers, §1.3) | 182,361 | **10** | ≤ ladder | 512 | 22,784 | 30,000 | **10** |

Each cell is the largest `n_ctx` at which that wall alone admits the row; the fit is the minimum.
Three things the table settles:

* **The dense row ships at the width the clock allows.** The ladder would take it to 651; the
  window stops it at 512 and refuses 513 by name (`CourtWindow`, and nothing else). A wider
  dense row on this ruleset is not a ladder question; it is an arity question (ADR-0092 D3), and
  at 128 lanes arity 64 fits the carrier — the generator's own derivation says what the arity
  would be at each width.
* **The hybrid — the structurally better long-context family (ADR-0081 §1.1: three layers in
  four carry O(1) state) — is held at 204 by one number, the ladder.** At 512 it is refused by
  the ladder ALONE; the window admits it, the chunks admit it. `palw_qwen36_profile` still says
  the row is 8 "because the whole-close derivation says so" — that was the 80 KiB carrier; the RC
  carries 27 and the binding wall moved without the comment moving.
* **The devnet's window admits the dense row to 4,096** (turn deadline 4, window 300), so its fit
  is the ladder's 651. A drill that wants a wider row than testnet-11 can carry has a network
  that can carry it, up to the ladder.

### 1.3 Kimi K3, as a stand-in, on the shipped ruleset

The public card (README and `config.json`, read 2026-09-10): 2.8T total / 104B activated
parameters, 93 layers as 69 KDA (a gated linear attention) + 24 gated MLA, hidden 7,168, 96
attention heads, MLA cache 512 + 64 a position, `v_head_dim` 128, 896 experts of 3,072 with 16
routed and 2 shared a token, vocabulary 163,840, context 1,048,576. Written as the hybrid
family's geometry (`palw_model_fit_v1::stand_ins::KIMI_K3_AS_HYBRID_V1`: KDA as GatedDeltaNet at
56 heads of 128, MLA as grouped-query attention at 6 KV heads of 128, 92 layers at interval 4 —
every substitution named on the constant, each in the direction that refuses rather than admits),
the tree's own predicates price it:

| n_ctx | what refuses |
|---|---|
| 1,048,576 (the card's) | the **geometry ceiling**, before a profile exists: `96,468,992 > 16,777,216`; the family builder refuses with `validate_geometry`'s own sentence |
| 131,072 | four walls at once — ladder `850,866,290,744 > 2^26`, window `3,618 > 2,999`, chunks `376,832 > 65,536`, public-da `524,288 > 120,000` — and the close unpriced |
| 512 | the ladder alone: `3,323,778,104 > 67,108,864` (50× over) |
| **10** | **admitted on every wall** |

So the honest sentence is not "the chain cannot carry a K3-class model". It is: **this ruleset
carries one at ten positions**, which is a class nobody would use, and every wider width is
refused by a named number — the ladder at 11, then four walls together past 32,768, then the
geometry ceiling past 182,361. No single fix admits it; Decision 5 says what would.

### 1.4 The 2M question, for every depth

`PALW_STEP_MAX_ENUMERATION` is a product, so the answer depends on nothing but the depth:

| positions | fewest layers refused | so |
|---|---|---|
| 2^21 (2M) | **9** | refused for every model deeper than eight layers |
| 2^20 (1M) | **17** | refused for every model deeper than sixteen |
| 2^17 (128K) | 129 | admitted for every model shallower than 129 layers — by THIS wall |

For the three geometries this tree prices the widest context the ceiling admits is 599,186 (28
layers), 419,430 (40) and 182,361 (92; 180,400 at the card's 93). The ceiling is a constant in
`validate_geometry` with a stated reason — it bounds an enumeration a validating node performs on
every `ClassRegistered` — and it is not the binding wall for any of them (§1.2: the ladder is),
so raising it alone would admit nothing.

### 1.5 What a seat must hold

From the geometry and the state map's row widths (i32 cache: `kv_heads × head_dim × 4` a
position a layer; the recurrence's state is `k_dim × v_dim × 4` a head and constant in the
context). No verdict: no ruleset states what a host has, and the artifact's size is the
converter's number — the parameter count is a lower bound at one byte a weight.

| candidate | n_ctx | attention cache | recurrent state | artifact |
|---|---|---|---|---|
| Qwen2.5-1.5B A16 | 512 / 2M | 28 MiB / 112 GiB | 0 | the converter's |
| Qwen3.6-35B-A3B | 512 / 2M | 20 MiB / 80 GiB | 60 MiB | the converter's (33 GiB, measured on 5f) |
| Kimi K3 stand-in | 512 / 1M / 2M | 69 MiB / 138 GiB / 276 GiB | 241.5 MiB | ≥ 2.5 TiB |

A seat replays a job from the prompt ids against the artifact it holds (ADR-0082 Decision 9,
ADR-0084/0086: the opening carries the fold, the seat supplies the leaves). At 1M positions a K3
seat holds a 138 GiB cache beside a 2.5 TiB artifact, and its replay must finish inside
`window_receipt × rate` (ADR-0082 D9 — the certification drill's number, measured at about one
second a token on the a16 host). That is not a wall in the tree; it is the reason Decision 5's
first item is the seat and not the court.

### 1.6 What the entrance told a client before

`GET /v1/models` carried `n_ctx` and nothing else about limits (`surface.rs`, `models_body`). A
request past the window was a 400 whose entire content was the worker's sentence — `prompt 51 +
decode ceiling 476 exceeds max_context_tokens 512` — which the Studio parses by string
(`backend/gateway.rs`, `ceiling_from_refusal`) to compute the retry it should have been able to
compute before sending. A `max_tokens` past `--max-decode-cap` was **clamped and nobody was told**
(`handle_chat`, `decode_limit`), which is the one silent downgrade ADR-0096 Decision 1's doctrine
left standing. Nothing said which features a fence had decided: a client learned that
`response_format` was advisory from the answer, and that `temperature` was refused from a 400.

### 1.7 The checklist

The operator's message carries a sixty-item list of what "a practical foundation for a
Kimi/Claude-class model" needs. Appendix A reads every item against this tree, with the file that
holds it, the ADR that decided it, or the doctrine that refuses it. The short form: the lane
already has the Tier-1 items an OpenAI-shaped client needs (the surface, streaming, tools as text,
formats, `usage`, conversations, the record, the model list — ADR-0096), refuses by doctrine the
ones that would make the seat's replay a different computation (sampling knobs, prompt/KV caching
across jobs, silent context truncation), and was missing exactly the two things this ADR adds:
the limits, published, and the fit, computed.

## 2. The requirement

> **R-fit — for any (model, context, ruleset), the answer to "does it fit" is a table generated
> from the chain's own predicates, with every wall named and its number, and the entrance a person
> uses publishes the limits their request must respect before they send it.** Nothing consensus
> moves to satisfy it. A wall that must move is a mint-time decision (ADR-0092 Decision 4) and is
> named here as one, never taken here.

## 3. Decisions

**Decision 1 — the fit is a lookup.** `consensus/core/src/palw_model_fit_v1.rs`:
`palw_model_fit_v1(profile, bundle, court, prompt_ids_form)` returns every wall of §1.1 as a row
`{wall, need, have, unit, verdict, note}` plus the answer-per-job and the seat footprint. Each
row is computed by the predicate admission runs — `worst_case_step_leaf_count_capped_v1`,
`derive_court_cost_shaped_v1` under the shape `palw_class_ladder_rules_for_court_v1` builds (the
genesis-anchored form for a class with no map), `palw_attn_court_admits_row_v1`,
`tiled_kv_state_geometry_v3` — never by a second spelling. `court` and `prompt_ids_form` are the
caller's reading of the fences (`palw_admission_shape_at_v1`, the acceptance path's own), for
ADR-0082 Decision 5's reason. A verdict is `Admitted`, `Refused` or `Unpriced`; the last is kept
apart because "too wide to price" and "priced and too wide" send a reader to different files.
`palw_geometry_ceiling_fit_v1(n_ctx, layers)` answers the first wall with no profile at all,
because it is the wall a person asking about 2M meets. `misaka-palw-base0 --bin palw-model-fit`
prints the table for a preset (§1's sections, in that order); it is the generator, and no ADR
writes a number it does not print.

**Decision 2 — the entrance says its limits before the first token.** `GET /v1/models` carries
`misaka.limits` and `GET /health` carries the same object (`surface::limits_body`, schema
`misaka.palw.limits.v1`): the context window and the rule on it (prompt and answer together), the
most an answer may be (`min(--max-decode-cap, n_ctx − 1)`), the default, the prompt's byte
ceiling, the tokenizer id and vocabulary (so a client that counts fetches the table ADR-0096
Decision 8 serves), `jobs_per_request: 1`, and — per feature the surface accepts — the word the
chain's fences decide: `advisory` / `committed` for formats, `greedy_only` / `requested` for the
sampler, `public_da` / `panel_da_available` and `flat` / `merkle` for the ids. The two refusals a
request meets before the chain — the window and the prompt bytes — are the gateway's own error
body (`{"error": {"message", "type"}}`, the sentence unchanged: every client reads
`error.message`) with `error.code` added — `context_length_exceeded`, which is OpenAI's own code
for the same refusal, so a stock SDK's `err.code` already reads it — and the numbers under
`misaka.refusal` (`room_for_answer` among them), so a client branches on a code and computes its
retry from numbers. And the clamp §1.6 found is reported: every answer carries
`misaka.decode = {requested_max_tokens, applied_limit, cap, clamped}`. Nothing is refused that was
served before, and nothing is downgraded that the response does not now say.

**Decision 3 — the geometry ceiling is a wall with a name, and this ADR does not move it.**
`PALW_STEP_MAX_ENUMERATION` refuses a 2M context for every model deeper than eight layers and a
1M context past sixteen (§1.4). It is a code constant, but it gates what a `ClassRegistered` may
carry, so a build that changed it would admit registrations another build refuses: it is a
ruleset move with the same shape as the ladder's, and ADR-0092 Decision 4 governs it — a network
that wants a wider product mints with one. It is also not the binding wall for any geometry this
tree prices (§1.2), so raising it alone admits nothing; a proposal to raise it must come with the
generator's table showing which wall binds after.

**Decision 4 — a stand-in is a verdict, never a row.** `palw_model_fit_v1::stand_ins` holds
geometries this tree prices that no chain registers, each substitution named on the constant and
chosen in the direction that refuses. No registration may cite a stand-in, no id derived from one
is a class id, and a stand-in that is ever converted becomes a MEASURED geometry in its family's
module and leaves this one. The K3 stand-in is the first; a second is added the same way, with
its card's reading dated.

**Decision 5 — what a network that wants a K3-class model at a long context must mint with, wall
by wall.** Named, not decided: each item is a mint-time number or a design with a measurement
in front of it, and this ADR's job is that the next author finds the list rather than one item.

| wall (§1.1) | what admits it | the ADR that owns it | what must be measured first |
|---|---|---|---|
| the seat (§1.5) | **a seat that holds a shard, not the model** — replay by expert or layer shard with the panel's coverage stated as a number | the successor of ADR-0080/0081 that ADR-0081 §3 waits for; ADR-0081 Decision 8's coverage number, never taken | the detection probability of one forged shard against the panel size (ADR-0081 U-03), and the replay clock against `window_receipt` (ADR-0082 D9) |
| geometry ceiling | a ceiling minted for the product the network wants | this ADR's Decision 3; ADR-0092 D4 | nothing — arithmetic; the generator prints the widest context per depth |
| ladder | `max_step_leaf_count` minted at the top of the wall-clock budget | ADR-0092 D1 | the generator's decision table (ADR-0092 §5) at the row's history |
| court window | an arity the carrier admits at the row's lanes; at 128 lanes 64 fits | ADR-0092 D3 (derived, never chosen) | `palw_court_arity_v1` at the registered set — the generator prints it per row |
| close | the responder that ADR-0093 specifies and no binary produces | ADR-0093 | ADR-0093 §7's order, unchanged |
| state chunks | a chunk cap sized with the ladder — the two are one budget, since both count the same cache | ADR-0082 D4's map; a re-mint moves `PALW_STEP_LEG_MAX_STATE_CHUNKS` with the ladder | the leg's serialization at the cap, measured |
| public-da payload | `PanelDa` for the ids (ADR-0077 D16, armed on the card only) or the tiled root (ADR-0081 D3, armed on the card from genesis) — testnet-11 has neither | ADR-0077 D16, ADR-0081 D3 | nothing new; both are built and fenced |
| the answer | ADR-0096 Decision 5's chain of jobs, as today; a longer job is `max_decode_tokens`, a ruleset field | ADR-0096 D5 | nothing |

What the table says as one sentence: **a K3-class network is a different mint, and the thing
that makes it possible is not a deeper court but a seat that does not hold the model** — the
court already dissects to one operation (ADR-0092 §2), and every other wall is a number the
generator can size. That successor is measured before it is designed (ADR-0081's own lesson), and
it is not this ADR.

**Decision 6 — what "practical use" means on this lane, and where each item lives.** Appendix A
is the decision: an item is *held* (with its file and ADR), *the app's* (MISAKA-Studio, under
ADR-0096), *refused by doctrine* (with the ADR that refuses it and why), or *absent and named*
(with what would carry it). Two items on the list are refused in a way worth stating here because
they look like features and are not: **prompt caching / KV handoff across requests** — the seat
replays a job from its prompt ids, a cached state is court material and not a transport (ADR-0096
Decision 5's last sentence), and a cache the seat cannot reach is a token the seat will not
reproduce; and **silent context compression** — the entrance trims and SAYS so
(`misaka.context.dropped_turns`), summarizes as a job of its own, and never fabricates a turn.

## 4. What this costs

* **Chain:** nothing. No object, no field, no fence, no id moves.
* **Node:** nothing at runtime; one module of pure functions and one test file. The generator
  runs three bisections per wall per candidate — about thirty seconds in a debug build for three
  candidates and two presets.
* **Gateway:** two JSON objects assembled on request; the refusal parse is two `split_once`s.
* **Wire:** `GET /v1/models` and `GET /health` grow by one object; the chat response by one small
  one. No field a client read before changes.

## 5. Invariants the tests must hold

```
1  Every row a shipped preset's genesis registers with a carried profile is ADMITTED on every
   wall of that preset (the positive control; read off genesis_objects, never off a list).
2  On the RC the dense row's fit is 512 and the wall at 513 is the court window alone; the
   ladder admits it to exactly 651, and past the ladder the close is UNPRICED (four walls), never
   admitted. The hybrid row at 512 is refused by the ladder ALONE, and the ladder admits it to 204.
3  A 2M context is refused by the geometry ceiling past eight layers and a 1M context past
   sixteen; the family builders refuse to build past it in validate_geometry's own sentence.
4  The K3 stand-in fits the RC at ten positions; at 512 the ladder alone refuses it; at 131,072
   four walls refuse it by name with the close unpriced; its seat footprint is the geometry's
   arithmetic.
5  A wall reads its ceiling: the same stand-in against the same ruleset with the ladder raised
   to 2^40 flips the ladder row to admitted, prices the close, and costs the window exactly
   2 × 14 × turn_deadline more (fourteen ladder rounds, both parties).
6  Every `have` is the bundle's own number read back, on every preset.
7  The limits object says the window, the answer ceiling (never the whole window, never past the
   operator's cap), the tokenizer, and — per feature — the word each fence decides; arming each
   fence moves exactly its own word.
8  A context refusal and a prompt-bytes refusal are the gateway's own error body plus
   error.code and misaka.refusal — remove those two and error_body(message) is what is left; every
   other message is error_body(message) byte for byte; the code is OpenAI's own where one exists.
```

1–6 are `consensus/core/tests/palw_adr0097_model_fit.rs` and the module's own tests; 7–8 are
`misaka-palw-gateway/src/surface.rs`'s. All were run red before green where a negative control
exists (5, 8) and observed against the generator's own output (1–4).

## 6. Order of work

1. Decision 1 — the module, the generator, the tests. **Done** (§9).
2. Decision 2 — the limits, the refusal, the decode report, the doc. **Done** (§9).
3. The Studio reads `misaka.limits` instead of `/health`'s bare `n_ctx` and branches on
   `misaka.refusal.code` instead of the sentence — MISAKA-Studio, under ADR-0096's branch pair.
4. `palw_qwen36_profile`'s "8 positions because the whole-close derivation says so" comment and
   `palw_qwen25_profile`'s "admits at most 574" are corrected to point at the generator; the
   numbers are the test's. **Done** (§9): each carries a dated note, the old sentence left in
   place beside it.
5. The generator's RC table is run on every re-mint and beside every class row a card registers
   (ADR-0092 §5's table gains its columns), so a class's fit is a lookup at mint too.
6. The successor: Decision 5's first row, measured first. Not on this branch.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0092 D1–D4, §5 | kept whole; the generator this ADR adds is the "table the next model's fit is a lookup in" §5 asked for |
| ADR-0092 §8's "at the wall" finding | restated as Invariant 2 from the fit rather than from the widest-ladder search; both stand |
| ADR-0081 §1.1 (the hybrid is the better long-context candidate) | confirmed and bounded: what holds it at 204 is the ladder alone (§1.2) |
| ADR-0081 Decision 8's coverage number | still not taken; Decision 5 names it as the first measurement of the successor |
| ADR-0077 Decision 13 (rows at 512 → 2,048 → 8,192) | 2,048 is refused today by the ladder AND the window (§1.2's sweep); the rungs are mint-time widths, as ADR-0092 D4 already says |
| ADR-0096 Decision 1 (refuse by name, never downgrade silently) | extended to the `max_tokens` clamp, which is now reported (Decision 2) |
| ADR-0096 Decision 5 (a long thread is a chain of jobs) | kept; §1.1 states the per-job answer bound beside it |
| `palw_qwen36_profile`'s and `palw_qwen25_profile`'s width comments | stale in the direction §1.2 measures; §6 item 4 |

## 8. What is deliberately not decided

* **Any wall's new value.** Each is a mint-time number (ADR-0092 D4), and the generator prices
  the candidates; the card's author reads the table.
* **The sharded seat** (Decision 5's first row). Measured first, designed after, under its own
  number.
* **Whether the geometry ceiling should be a ruleset field** rather than a constant. It behaves
  as one (Decision 3); making it a field is a fingerprint move for a value nobody has asked to
  change on a running chain.
* **A `/v1/tokenize` route.** The gateway has no tokenizer (`wire.rs`, by design: the worker
  renders); the tokenizer id and the served table (ADR-0096 D8) are what a client counts with.
* **Kimi K3's actual KDA head count and MLA-as-cache pricing.** The stand-in's substitutions are
  named and one-directional; a measured geometry replaces it the day the model is converted.

## 9. Number hygiene and implementation record

0097 is the next free number in the index at `70d56519` (ADR-0096's README row says so; 0094 and
0095 are resident on `main`), and this file claims it on `feat/adr-0097-model-fit` — written and
measured at `70d56519`, then rebased the same day onto `421cc92e` (`feat/adr-0096-everyday-lane`'s
merge of `origin/main` at `7f4dded4`), where the generator's output was re-run and is byte-identical
to §9's record. A concurrent claimant renumbers the later writer. **The next free number is 0098.**

* **2026-09-10** — ADR written against `70d56519` and rebased onto `421cc92e` (numbers unchanged);
  implemented on the same branch the same day:
  `consensus/core/src/palw_model_fit_v1.rs` (Decision 1: the walls, the report, the geometry
  ceiling alone, the stand-ins module with `KIMI_K3_AS_HYBRID_V1`),
  `misaka-palw-base0/src/bin/palw-model-fit.rs` (the generator: genesis rows, the sweep, the
  widest context per wall, the 2M table, the seat), `consensus/core/tests/palw_adr0097_model_fit.rs`
  (Invariants 1–6; 7 tests, 2 in the module), `misaka-palw-gateway/src/surface.rs` +
  `main.rs` (Decision 2: `limits_body`, `refusal_body`, `context_refusal`,
  `prompt_bytes_refusal`, `misaka.decode`; two new tests and the model-list test extended, the
  corpus unchanged at `5cdf9288…`), `docs/palw-freeprompt-gateway.md` (the limits and the refusal), and
  `docs/palw-model-fit-testnet11-2026-09-10.md` (the generator's RC output, as a dated record and
  not as a source). **Corrected the same day, by the Studio half (§6 item 3):** the first
  `refusal_body` served `{"error": "<sentence>"}` — a string where the gateway's own `error_body`,
  every OpenAI SDK and the Studio's `Lane::send` read an object — and its test compared the
  function with itself, so it passed. It now builds on the one spelling (`surface::error_body`,
  which `main.rs` delegates to), puts the code in `error.code`, and the test asserts that removing
  what the refusal adds leaves `error_body(message)`. Measured on the way, and recorded in §1
  because none of it was in `docs/`:
  the dense row's ladder width is 651 (not the 574 the profile comment says), the hybrid row is
  held at 204 by the ladder alone, the devnet cannot carry the hybrid row at all (one carrier),
  and the K3 stand-in fits at ten positions.

## Appendix A — the sixty-item checklist against this tree

The operator's list, by tier, with where each item is. *held* = in this tree, with its place;
*app* = MISAKA-Studio under ADR-0096; *doctrine* = refused on purpose, with the ADR; *named* =
absent, and what would carry it.

**Tier 1 — what an OpenAI-shaped client needs**

| item | where |
|---|---|
| 1 context length | held — the class's `n_ctx`, published now as `misaka.limits.context_window` (D2); its walls §1.1 |
| 2 input limit | held — `prompt + decode ≤ n_ctx` (`fp_worker.rs`); `max_prompt_bytes`; refusal with a code (D2) |
| 3 output limit | held — `max_output_tokens` (D2); `finish_reason: length`; ADR-0096 D5's continue legs (app) |
| 4 context management | app — trim → summary job → continue, each a claim, all reported (`misaka.context`, `misaka.jobs[]`, ADR-0096 D5) |
| 5 repository understanding, 6 code RAG, 7 AST, 8 git, 9 issues/PRs, 10 documents | not this lane's: an agent's tools over a repository are an application above the entrance; nothing on chain knows what a repository is (ADR-0096 D2's last sentence, for tools) |
| 11 long-term memory, 12 working memory | app — conversations as the runtime's files with export/import (ADR-0096 D12); the record (`records.rs`) |
| 13 tool calling | held — tools as the model's own Hermes-style text, `tool_calls[]` parsed after the run, `tool_choice` advisory (ADR-0096 D2); the round-trip is the app's |
| 14 agent loop | app — one job per leg, `misaka.jobs[]` (ADR-0096 D2/D5) |
| 15 sandbox, 16 permissions, 17 human approval | held for the HOST — ADR-0079 (the worker's confinement, the reachable-secret check, the public-bind acknowledgement); an agent's own permission levels are the app's |
| 18 streaming | held — SSE, the committed ids re-checked against what streamed (ADR-0077 D2 / W5) |
| 19 partial response / resume | app — `finish_reason: length` → continue legs (ADR-0096 D5); a job never resumes: one inference, one claim (R0) |
| 20 context compression | app — the summary job (ADR-0096 D5); **doctrine**: never silent, never fabricated |
| 21 prompt caching | **doctrine** — a cached state is court material, not a transport; the seat replays from the ids (ADR-0096 D5, ADR-0082 D4). What IS cheap: the artifact is resident (ADR-0077 D1) |
| 22–23 model switching / router | app — the Studio's backends (llama.cpp, MLX, the gateway); a gateway serves ONE class (`/v1/models`, ADR-0096 D1) and a router is a list of gateways |
| 24 modalities | **doctrine** — non-text parts refused by name (ADR-0096 D1); a class is a text graph |
| 25 error/log context, 26 runtime context, 27 chain context | held for the CHAIN — `/health`'s four names and `misaka.chain` (ADR-0077 D3); misakascan; `misaka palw facts`. As agent tools: the app's |
| 28 security context, 29 consensus safety | held — the court, the fences, `consensus-changes-by-activation`; a model does not change consensus by asking |
| 30–33 tests, fuzzing, property checks, diff review | held for THIS tree (the KATs, the profile fuzzers, the drills); as agent tools: the app's |
| 34 multi-agent | app |
| 35–36 context priority / budget | app — ADR-0096 D5's order (system, newest, then oldest-first drops); the budget is `n_ctx` and D2 publishes it |
| 37 token counting | held in part — `usage`, `misaka.decode`, `prompt_tokens` after the run; before it, the tokenizer id (D2) and the served table (ADR-0096 D8); no `/v1/tokenize` (§8) |
| 38 capability registry | held — `misaka.limits` (D2) on `/v1/models`; the class table and the components manifest (ADR-0096 D10) |
| 39 OpenAI/Anthropic-compatible API | held — the OpenAI surface (ADR-0096 D1); no Anthropic shape (named, not built) |
| 40 provider separation, 41 AI gateway | held — the gateway IS the adapter (`misaka-palw-gateway`); the chain never sees a provider |
| 42 audit log, 43 reproducibility | held — the record (`records.rs`, app), the artifact JSON beside every job (`fp-v3-gateway-artifact.v1`), and the claim itself: model, prompt ids, ids, roots, ruleset id — a commitment IS a reproducibility record |
| 44–46 cost, latency, fallback | app — the backends; on chain the price is leaves (ADR-0074 D5) and the latency is the row (§1.5) |
| 47 local/private model | held — `--answer-never-commit` with a bond-less identity (ADR-0096 D10); `PanelDa` for the ids where armed (ADR-0077 D16) |
| 48 secret redaction | held in part — SA-7 (the worker's stderr withheld), the reachable-secret boot check (ADR-0079); prompt content is the person's, and `PublicDa` says so beside the box (ADR-0096 D7's last bullet) |
| 49–51 prompt-injection, tool-result injection, isolation | app — the entrance renders a tool's reply as a `user`-role text turn (ADR-0096 D2), which is the isolation the model's own template offers and no more; nothing on chain executes a tool |
| 52–56 chunking, retrieval, reranking, relevance, large files | app / not this lane's |
| 57–58 structured tool output, typed tools | held — `tools` as JSON under RFC 8785 (ADR-0096 D2), the JSON-Schema subset (D3/D7) |
| 59 agent state machine | app |
| 60 the target architecture | the entrance (ADR-0096) over the lane (ADR-0077) over the court; the "gateway" of the diagram is this repository's gateway and the "runtime" is the Studio |

**What the list asks for that the lane refuses on purpose**, in one place: a sampler the seat does
not replay (ADR-0082 D11), a cache across jobs (ADR-0096 D5), a silently shortened context
(ADR-0096 D1), a modality the graph does not compute (ADR-0096 D1). **What it asks for that was
missing and is now here**: the limits (D2) and the fit (D1). **What it asks for that is named and
not built**: an Anthropic-shaped surface, a `/v1/tokenize` route, and the sharded seat that a
K3-class network would stand on (D5).
