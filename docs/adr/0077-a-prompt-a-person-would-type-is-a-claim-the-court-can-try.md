# ADR-0077: A prompt a person would type is a claim the court can try

**Status:** PROPOSED (2026-09-02). Written against a measurement, not a design review: the
network's stated purpose is a person using their own local LLM on a prompt of their own, with that
one inference mining (ADR-0044 §Context, ADR-0073 §1), and on the live testnet-11 (Relaunch 5e)
the widest such prompt a registered class admits is **eight tokens, answer included**, on a lane
that holds 100‰ of the cadence, weighs nothing as a block, and has never been run on the fleet.
Nothing here is armed by this document. Its one requirement is R0 (§2): the practical local
LLM and the mining runtime are the SAME inference — not two binaries that each hold half of the
purpose. Phase A is executor-side and consensus-inert. Phase B
moves the ruleset (a re-genesis on testnet-11, as every relaunch has been; mainnet ships PALW off
and is untouched). Phase C is ADR-0073 Decision 4's activation, sequenced here and not re-decided.
Phase D is refused by admission until the ruleset move that carries its rules.
**Builds on:** ADR-0044 (the receipt lane; F1–F16), ADR-0049 Decisions C/E/F (admission bounds
the court from the class's geometry; decode is adjudicable through the tiled logits pin; engine,
profile, adjudicator and inventory are one description), ADR-0070 (the model tiers' step spaces
are adjudicable end to end), ADR-0073 (Phase ① landed; Decision 2 decided; Phase ③ landed through
ADR-0074 Decision 5; Phase ④ pending), ADR-0074 Decisions 1 and 5 (`User` and `Canonical` prompt
modes; the quantum is an eighth of the canonical job), ADR-0075 (certification is a consensus
object; a class is seated by `ClassLaneCertified`), ADR-0076 (the attempt seed is `share · pwu`;
the receipt lane seeds on its own).
**Amends:** ADR-0044 Decision 8 (PublicDA is the only weight-bearing mode) and Decision 10 (v1 is
non-streaming); the "streaming" and "encrypted prompts" rows of ADR-0044's *not decided* list.

## 1. What was measured

The three classes testnet-11 registers, read off `canonical_classes_v1` /
`qwen36_canonical_classes_v1` and the profiles they project:

| class | model | `n_ctx` (prompt + answer, total) | free-prompt worker | canonical job | `pwu_per_inference` |
|---|---|---|---|---|---|
| PALW-BASE-0 `f1c5635c…` | integer floor — not a language model | 12 | none | (11, 2) | 7,708 |
| PALW-QWEN25-A16 `71bbb755…` | Qwen2.5-1.5B, graph-v2 | 16 | `palw-a16-fp-worker` | (14, 2) | 1,589,424 |
| PALW-QWEN36 `5bd9ae3d…` | Qwen3.6-35B-A3B, graph-v3 | 8 | `palw-qwen36-fp-worker` | (7, 2) | 2,685,360 |

Against those widths, the gateway's frozen plain-marker template costs ~9 tokens by itself
(`docs/palw-fp-on-registered-classes.md`, measured on the A16 tokenizer). On the hybrid the
template alone does not fit and `palw-qwen36-fp-worker` refuses the job at its own length check;
on the dense tier a browser prompt is "a few tokens each way — the pipeline proof, not the product
width". The worker binaries map the artifact **per job** (`load()` inside `run_job`; ~8 minutes
for the 33 GiB hybrid artifact, the 5e runbook's own number), the gateway spawns a process per
request and refuses `stream: true`, and no drill, test or runbook runs gateway → worker → node →
panel → receipt block: the 5e fleet table has no gateway, `testnet11-join-mining.md` does not
contain the word "prompt", two of the three FP smoke scripts pass the gateway a flag it no longer
accepts, and the llama.cpp worker all three drive omits two fields the wire type now requires and returns the
null execution root the transition refuses (`UnadjudicableCommitment`).

On the chain side, a certified claim is spendable at `final_daa + receipt_maturity` — with
`anchor_delay` 20, `window_challenge` 1,200 and `receipt_maturity` 400 that is ~1,620 DAA after
the commitment lands, **~54 hours** at the frozen 120 s cadence; the receipt lane holds 100‰ of
cadence (`ATTEMPT_SHARE_PERMILLE = 900`); and a receipt block carries no chain position
(`algo_id_carries_no_chain_position(7)` is `true`, ADR-0073 Decision 4a unlanded). Prompts are
public (`PublicDa` is the only mode the worker executes). Mainnet has no PALW at all.

### Why eight and sixteen: the ceilings are the court's, and both are ruleset facts

`verify_class_admission_v2` admits a class only if its worst close fits the RC court, and
`derive_court_cost_v1` derives that worst close **over the class's longest job** — the job an
attacker would pick. Three of its terms are linear in `n_ctx`:

* the KV history a `MatMulQuant` at an attention site reads — one range run per position, each
  `kv_dim` wide, each with its own bounded sibling set;
* the recurrence's replay — `positions = n_ctx` for every `GatedDeltaNet` ref, because the
  shipped replay is genesis-anchored: `gdn_core_genesis_replay` walks every prior position, and
  the class registers no state chunk map (`state_chunk_map_id` is the sentinel; the profile's own
  comment: "the anchor consumption is wired for attention and not yet for the recurrence");
* the prompt ids and the generated-id pin, `n_ctx × 4` each, on every close that addresses a gather.

The bound is `DEFAULT_MAX_CLOSE_BYTES = 80 KiB`, the mempool's standard-transaction mass mirrored,
because a close is a transaction. Eight positions of the hybrid hold its worst close at ~90 % of
that (the profile's derivation); n_ctx 20 is the last value the dense tier's close admits.

Independently, `worst_case_step_leaf_count_v1` enumerates `n_ctx − 1` prefill positions and one
decode call and refuses anything above `PALW_STEP_MAX_LEAVES = 2^22`. The hybrid's canonical (7, 2)
counts 2,685,360 leaves, ~298 k per position, so the ladder alone admits ~14 positions of it; the
dense tier's (14, 2) counts 1,589,424, ~99 k per position, ~42. The cap's own comment records that
it was "chosen when BASE-0's graph was eighteen steps per layer" and that "sizing a ladder to the
class set of the day is how that happens". It is inside `palw_ruleset_id_v2`.

Neither ceiling is a bug in an executor. A class's width IS the width its court can try, so
widening the lane is a court change first, and the executor-side gaps are a product that has never
been assembled. This ADR does both, in that order.

## 2. The requirement, and the principle

Two things exist in this tree today, separately. `misaka-palw-serve` is a practical local LLM: the
same A16 engine a class registers, speaking OpenAI at the artifact's full 512 positions — and its
own header says a run served there is "NOT a claim anyone can adjudicate or mine"
(`court_capable: false`). The family workers behind the gateway mine with a prompt — and admit 8
or 16 positions. Each is real; neither is the purpose, and a tree that keeps both is a tree in
which "practical" and "mines" are two products that never meet. So this ADR has one requirement,
and every decision below is in its service:

> **R0 — one inference, at the width a person uses, is one answer and one claim.** There is one
> runtime; it answers at the width the class registers; every answer it gives is captured and
> committed by the same run; and the only reasons a commitment does not reach the chain are the
> chain's — no bond, no exposure room, a class the chain does not certify — never a runtime mode.
> "Practical" is therefore a property of the class table (Phase B), not of a side binary, and
> "mines" is a property of every served inference (Phase A), not of a narrow one. Neither phase
> is done on its own: R0 is measured on ONE job id (§6).

**The principle that delivers R0:** the court's cost is a function of the checkpoint interval,
never of the context; the ladder is sized to a prompt a person would type; and everything the
person touches is one pipeline that a drill runs end to end. Nothing about how a beacon is drawn, how a panel is seated, how a ticket
is compared or how a claim is disputed changes — F4, F5, F6 and F15 of ADR-0044 hold verbatim,
which is the whole reason a wide prompt is safe here and forbidden on the attempt lane (the win is
a quantum ticket against the class's receipt target; the prompt does not decide the lottery).

## 3. Decisions

### Phase A — the executor side, consensus-inert

**Decision 1 — one runtime: the server is the worker.** `misaka-palw-serve` is retired and its
role moves into the family workers, which gain `--mode v3-serve`: the artifact is mapped once, the
manifest handshake happens once, and jobs arrive as the SAME framed `PalwFpWorkerRequestV3` /
`PalwFpWorkerResultV3` frames over a persistent stream, one generation at a time (a single engine
and a single KV cache — the rule `misaka-palw-serve` stated, kept). Every job it answers is
captured; there is no un-captured chat path left in the tree, and `court_capable: false` ceases to
be a state a runtime can be in. The served width is the class's registered `n_ctx`, read from the
catalog row, never the artifact's rotary span: a runtime that answered wider than the court admits
would be exactly the two-products split R0 exists to close — which is also why Phase A alone does
not satisfy R0, and Phase B is what makes the one width a practical one. Per-job retention
(`material.bin`, the manifest) is unchanged, and `v3-job` stays as the one-shot form the drills and
the replay arm use. Pinned: a job's roots through `v3-serve` are byte-identical to the same job's
roots through `v3-job`.

**Decision 2 — the answer streams; the commitment does not.** The gateway accepts
`stream: true` and forwards tokens as the worker decodes them (a side channel the worker writes
beside its result frame). The commitment exists only at completion, as ADR-0044 Decision 10 says.
What binds the two: the gateway re-renders the committed `output_token_ids` when the frame arrives
and, if the rendering is not the bytes it streamed, closes the stream with an error and writes NO
commitment — a worker that shows one answer and commits another is a worker whose commitment is
not the user's inference, and F1 lives or dies on that. Streaming is UX; the consensus object is
untouched.

**Decision 3 — the gateway reads the chain it commits to.** Today it reads a hand-refreshed
`anchor.json` and an `identity.json` whose `class_id` nothing checks (ADR-0075 §4 records the gap).
It now reads, from its node over RPC: the class registry, the free-prompt-certified set (genesis
and `ClassLaneCertified`), the executor bond's status and exposure room, and a fresh anchor. A job
on a class the chain does not certify is still answered — the answer is the product — but its
commitment is marked unsubmittable and never leaves the outbox, and `/health` says so by name:
`registered`, `fp_certified`, `bond_active`, `exposure_room`. A gateway with no bond at all is
served the same way: the answer, a capture, a commitment, and the chain-side reason it stays in
the outbox — R0's "never a runtime mode", made visible.

**Decision 4 — one handoff: answer, commitment, signature, submission, capture.** After the
frame: the commitment is signed (the `kaspa-pq-signer` sidecar, or a local seed for devnets — the
rail's two forms, unchanged), the 0x4a transaction is submitted through the node, and the capture
is staged into the node's retention directory as `<claim>.material` in the same step —
automatically, per job, with retention until the claim retires (`CLAIM_RETIREMENT`). The gateway
calls the library the CLI's `fp-submit` calls; it does not shell out to it, and the CLI command
stays as the manual form. A job whose bond has no exposure room for one more claim is answered and
queued, not submitted and refused at the transition.

**Decision 5 — the family workers are the gateway's workers; the llama arms go.**
`misaka-palw-worker`'s `v3-manifest` / `v3-job` modes are deleted: consensus refuses their null
execution root, no registered class runs on that runtime (ADR-0053), and the crate is outside
`default-members` so nothing noticed it stop compiling. The three FP smokes are pointed at
`palw-a16-fp-worker`, given the gateway's current flags, and one of them runs in CI against a
fixture-sized A16 artifact; the pinned 1.7 GiB artifact is the drill's, not CI's.

**Decision 6 — the template speaks the model's own control tokens, segment-wise.** The v1
plain-marker template exists because a tokenizer running with `parse_special = false` would
tokenize ChatML markers as prose. The family tokenizer encodes SEGMENTS: markers are emitted as
the model's special ids by the gateway, user text is encoded with specials disabled, and the two
are concatenated as ids — untrusted text can never smuggle a control token, and the model sees the
template it was trained on, so EOG fires and an answer ends where it ends instead of at the
ceiling. The template id moves (`…/chat-segments/v1`); consensus sees ids only, so this is Phase A.

**Decision 7 — the drill is a chain, not a harness.** `misaka-palw-fp-devnet-e2e.sh`, shaped
like `misaka-palw-certify-devnet-e2e.sh`: N validators from one build, one of them a gateway with
a family worker under a devnet bond, a browser-shaped request, then every node read back for the
same `FreePromptCommitted`, the same `PanelBound`, the same `ReceiptLicensed`, `Final`, and a
receipt block accepted by all of them. It runs on a devnet preset whose windows are minutes, because
the in-harness finding stands — a single-chain `TestConsensus` does not accrue the DAA the windows
need — and a multi-node chain does. This drill is the gate for arming Phase B on testnet-11.

**Decision 8 — the public pages say how.** `testnet11-join-mining.md` gains "mining with your own
prompts" (the bond, the worker, the gateway, what is public, and the four stages `submitted`,
`bound`, `certified`, `spent` shown by name — a block does not follow a prompt, and any interface
on this lane says so). The stale headers are corrected: `palw-fp-on-registered-classes.md` ("Nothing
here is implemented"), `palw-freeprompt-gateway.md` ("none does yet, by design"),
`palw-fp-wiring-atomicity.md` ("not started").

### Phase B — the court prices the checkpoint, not the context (one ruleset move)

**Decision 9 — history is replayed from a checkpoint, on both kinds of layer.** For a class that
registers a state chunk map, a refutation at position `p` opens the checkpoint chunk at
`c = ⌊(p − 1) / interval⌋ · interval` and replays at most `interval` positions after it; the
genesis-anchored long form is REFUSED for such a class, so a challenger cannot pick whichever route
convicts (the equivalence the floor's test already holds: anchored and long reach the same
verdict). This is what the attention path does today under the v2 map geometry (the corrected A16
sweep exercises it). The decision extends it to the recurrence: the hybrid registers a state chunk
map whose rows are one head's `k_dim × v_dim` state plus the conv window
(`integer_kv_state_chunk_map_id_v2` is the shape for the dense cache; the recurrence gets its own
layout id, in the checkpoint profile and therefore in the class id), the worker captures the leg
against it, and `gdn_core_genesis_replay` gains its anchored twin.

**Decision 10 — admission prices the checkpoint interval, not the context.**
`derive_court_cost_v1` prices the anchored form for a class with a map: KV refs and the
recurrence's `positions` run over `min(n_ctx, interval)` positions plus ONE checkpoint-chunk
opening per history-reading ref; terminal MACs likewise. The pessimistic price of the checkpoint
sentinel stays for a class without a map — such a class cannot widen, and a low price would read as
approval. Consequence: `max_close_bytes` becomes a function of `interval`, and `interval` becomes
the one court knob a class registers that its context does not move.

**Decision 11 — the ladder is sized to a typed prompt.** `COURT_MAX_STEP_LEAVES` moves from
`2^22` to `2^30`. The gate stays the rule: `(⌈log₂ leaves⌉ + terminal) × turn_deadline <
window_court`, i.e. `(30 + 2) × 60 = 1,920 < 3,000`. At today's per-position counts `2^30` leaves
is ~3,600 positions of the hybrid and ~10,800 of the dense tier; the per-position count grows
slowly with context (the attention reductions' `KvScaled` widths), so the class table, not this
ADR, states each row's admitted `n_ctx`, and the derivation refuses what does not fit, as it does
now. The constant is inside the ruleset id and its comment says it "cannot be raised afterwards";
that is correct, and a ruleset move is what a testnet-11 relaunch is.

**Decision 12 — the context ladder: rows at the artifact's rotary span, by the ADR-0075 route.**
The dense artifact's rotary table covers 512 positions (`max_position` 512, the converter's
default); the hybrid's "still covers 512". So the first practical rows are
`Qwen/Qwen2.5-1.5B/graph-v2` at `n_ctx` 512 and `Qwen3.6-35B-A3B/graph-v3` at `n_ctx` 512 — each a
NEW class id, because a class IS its graph, registered and seated through `palw-certify drill|bind`
and `misaka-cli palw submit-object` once Phase B's court is the shipped one. 512 is the width
`misaka-palw-serve` serves today, so this is the number at which the practical runtime and the
mineable one become one row — R0's width. The 8- and 16-token rows stay on chain exactly as they
are. Wider than 512 needs a re-converted artifact with a wider
table, and takes the same route with no further consensus change.

**Decision 13 — the canonical job grows with the context, so the cap stays a jackpot bound.** A
quantum is `pwu_per_inference / 8` leaves (ADR-0074 Decision 5) and a receipt is capped at 64 quanta
(`MAX_QUANTA_PER_RECEIPT`). With the hybrid's (7, 2) canonical job, a 512-token job is ~450 quanta,
capped to 64 — 86 % of real work certified and uncounted. The cap is the per-receipt jackpot bound
ADR-0044 Decision 5 meant; it must not become a tax on ordinary use. So registration requires the
canonical job's footprint to be at least `n_ctx / 8`: the widest admissible job then earns at most
`8 × 8 = 64` quanta by construction, and `verify_class_admission_v2` refuses a row that violates
it. Consequences, stated: `pwu_per_inference` grows with the canonical job; the attempt seed follows
it (ADR-0076 Decision 1 — cadence is unchanged, the draw rate falls and the seed compensates); a
claim's reserved exposure `pwu_per_inference × slash_value_per_pwu` grows with it, so the bond floor
a producer needs is re-derived by the existing rule, and a canonical hybrid inference at
1.75 tok/s takes ~40 s for a 64-position job instead of ~9 s — inside the 120 s cadence.

### Phase C — the weight (sequenced, not re-decided)

**Decision 14 — weight follows measured supply.** ADR-0073 Decision 4 (a receipt block has a
chain position; the share leaves 900‰ on a schedule bounded by what the lane produces) activates
when a Phase-B class has produced receipt blocks on testnet-11 through Decision 7's path for one
full retarget span, so 4b's schedule has a supply to follow rather than a promise. Until then a
spent quantum adds exactly what it adds today. Order, as ADR-0073 states it: court, price, weight,
share — this ADR sits between price and weight.

### Phase D — a prompt that is not published

**Decision 15 — `PanelDa` (privacy mode 2).** The job carries `prompt_token_ids_hash` as it does
now; the commitment transaction carries NO ids. The ids travel with the capture the executor
already serves to its panel (`<claim>.material`, `request_palw_material`); a seat checks
`H(ids) == prompt_token_ids_hash` before it reads anything else, and files `Valid` only for a claim
whose ids it holds. Withholding is the two-sided quorum's `ProducerDefaulted` arm — the same arm
capture withholding reaches today (ADR-0073 Phase ① 1a/1e) — and a court close that addresses a
gather carries the ids as it does now, so a disputed prompt becomes public. The honest name for
this is *private unless disputed*: five seats see the prompt, a dispute publishes it, and nothing
here is confidentiality. The gateway shows the mode and that sentence before first use (ADR-0044
Decision 8's obligation, carried). Weight: the same as `PublicDa` — the panel still replays from
data it holds, which is what the weight rests on. Admission refuses mode 2 until the ruleset move
that carries these rules, exactly as the worker refuses every non-PublicDA mode today ("a mode the
panel cannot replay must not execute").

## 4. What this costs, stated before it is measured

* **Capture size.** A 512-position capture commits 512 positions of rows; seats verify by hashing
  and `k` sampled leaves (ADR-0073 1e), so verification stays `O(k)`, but the bytes a seat fetches
  scale with the job. To be measured in the Decision 7 drill; the bound to hold is the retention
  budget, not the panel's time.
* **Commitment bytes.** `PublicDa` carries `n_ctx × 4` bytes of ids — 2 KiB at 512 — inside a
  standard transaction. `PanelDa` carries none.
* **Court time.** A rung is a turn pair at 60 DAA; 32 rounds is ~1,920 DAA, ~64 hours worst-case
  honest prosecution, inside `WINDOW_COURT`. `MAX_CLAIM_EXPOSURE_DAA` is unchanged.
* **Executor time.** The hybrid decodes at ~1.75 tok/s on a 24 GiB M4 Pro: a 300-token answer is
  ~3 minutes, streamed. The dense tier is interactive (~30 tok/s). Integer GPU kernels remain the
  practical-runtime plan's next stage (§8).
* **Bonds.** Decision 13 raises per-claim exposure by the canonical job's growth; a producer on a
  practical row needs the collateral the existing rule derives for it.
* **Identity.** Phase B moves every V2 preset's fingerprint: a re-genesis on testnet-11. Mainnet is
  byte-for-byte untouched (PALW off). Phases A and D move nothing until their own gates.

## 5. Invariants the tests must hold

```
W0   R0: every answer the runtime gives has a capture and a commitment from the same run; the
     served n_ctx equals the class's registered n_ctx; a commitment that does not reach the chain
     names a chain-side reason (no bond, no exposure room, uncertified class) in /health; no
     binary in the tree answers a chat without a capture.
W1   For a class with a registered state chunk map, derive_court_cost_v1 at n_ctx = interval,
     2·interval and 8·interval yields the same max_close_bytes and max_terminal_macs.
W2   The anchored refutation and the long form reach the same verdict on honest material, for
     attention AND the recurrence; the long form is refused for a mapped class.
W3   The widest job a class admits earns ≤ max_quanta_per_receipt quanta; registration refuses
     a row whose canonical footprint is under n_ctx / 8.
W4   (⌈log₂ COURT_MAX_STEP_LEAVES⌉ + terminal) × turn_deadline < window_court, at every preset.
W5   The streamed bytes equal the rendering of the committed output ids, or no commitment is
     written (one inference, one commitment — F1 through the stream).
W6   A job's roots through v3-serve equal its roots through v3-job, byte for byte.
W7   The devnet drill ends in a receipt block accepted by every node, on a chain whose DAA has
     advanced through bind, challenge and maturity.
W8   PanelDa: a seat holding no ids cannot file Valid; a hash mismatch is refused by name;
     withholding reaches ProducerDefaulted; a court close still carries the ids.
W9   No decision here changes how a beacon, a panel or a ticket is derived: ADR-0044 F4–F6 and
     F15 hold verbatim, pinned by the existing golden vectors.
```

## 6. Order of work

| unit | content | done when |
|---|---|---|
| P-01 | Decision 1 — `v3-serve` on both family workers; `misaka-palw-serve` retired | W0 and W6 green; the hybrid answers a second job without re-mapping |
| P-02 | Decision 2 — SSE side channel + the re-render check | W5 green, with a deliberately mismatching worker refused |
| P-03 | Decision 3 — the gateway reads registry, certified set, bond, anchor over RPC | `/health` names all four; an uncertified class answers and never submits |
| P-04 | Decision 4 — sign, submit, stage capture, retain; shared with the CLI | a browser request ends in `FreePromptCommitted` on a devnet node with `<claim>.material` present |
| P-05 | Decision 5 — llama v3 arms deleted; smokes repointed; one in CI | `cargo build` + the CI smoke green on a fixture artifact |
| P-06 | Decision 6 — segment-wise chat template | EOG observed on the dense tier; the F1 golden test extended to specials |
| P-07 | Decision 7 — the devnet drill | W7 green on three nodes |
| P-08 | Decision 8 — pages | the join-mining page has the section; the three stale headers are gone |
| P-09 | Decision 9 — recurrence state map + anchored replay | W2 green for `GatedDeltaNet` |
| P-10 | Decision 10 — `derive_court_cost_v1` prices the interval | W1 green |
| P-11 | Decision 11 — `COURT_MAX_STEP_LEAVES = 2^30` | W4 green; ruleset id moves once for P-09..P-13 |
| P-12 | Decision 12 — the 512 rows, drilled and bound on devnet, then testnet-11 | both rows `ClassLaneCertified` on the free-prompt lane |
| P-13 | Decision 13 — the footprint rule at registration | W3 green |
| P-14 | Decision 14 — ADR-0073 Phase ④ activation | one retarget span of measured receipt supply on a 512 row |
| P-15 | Decision 15 — `PanelDa` | W8 green; drilled; its own ruleset move |

**Done when** the Decision 7 drill, on a Decision 12 row, shows ONE job id in three places: the
gateway's streamed answer to a ~300-token chat turn, the node's `FreePromptCommitted` → `Final`
for that claim, and a receipt block spending one of its quanta, accepted by every node. R0 is a
property of a single inference, so it is measured on a single job id — never on one log of a
practical runtime beside another log of a mining one. Until that job id exists, this ADR is not
implemented, whatever else has landed.

## 7. Supersession

| what | disposition |
|---|---|
| ADR-0044 Decision 8 — `PublicDa` is the only weight-bearing mode | amended by Decision 15: `PanelDa` bears weight once armed; encrypted modes stay a future ADR |
| ADR-0044 Decision 10 — v1 is non-streaming | amended by Decision 2 |
| ADR-0044 *not decided* — streaming | decided (Decision 2) |
| ADR-0044 *not decided* — encrypted prompts | partly decided (Decision 15 is served, not encrypted); ML-KEM/ZK forms remain not decided |
| ADR-0044 Decision 5 — `max_quanta_per_receipt` bounds the jackpot | honoured; Decision 13 keeps it a jackpot bound rather than a tax |
| ADR-0049 Decision C — admission bounds the court from the geometry | honoured; the bound's form changes (Decision 10) |
| ADR-0073 Decision 4 — receipts gain position and share | sequenced by Decision 14, not re-decided |
| ADR-0074 Decision 5 — the quantum is an eighth of the canonical job | honoured; Decision 13 sizes the canonical job |
| `misaka-palw-serve` — "a run served here is … NOT a claim anyone can adjudicate or mine" | retired by Decision 1; no un-captured serving path remains |
| `palw_qwen36_profile` — "n_ctx 8 … a larger context returns when the recurrence's replay is checkpoint-anchored" | that return is Decisions 9–12; the row itself is unchanged |
| `palw_fp_devnet_v3::COURT_MAX_STEP_LEAVES` — "cannot be raised afterwards" | correct; raised by a ruleset move (Decision 11) |

## 8. What is deliberately not decided

* **Integer GPU kernels.** ADR-0053 removed the float Metal path; no integer GPU kernel exists.
  The hybrid stays a streamed, minutes-per-answer tier until the practical-runtime plan's next
  stage lands. A class's speed is a host fact and never reaches consensus.
* **KV continuation across turns** (`ContinueFrom{parent_receipt}`). Each request re-sends its
  history and pays prefill for it; correct, and not cheap. Its own trace semantics, its own ADR.
* **The windows.** ~54 hours from commitment to spendability is bind, challenge and maturity —
  fraud-proof safety, not a UX knob. The product shows the stages by name instead.
* **Private eligibility and receipt transfer** — ADR-0044's list stands.
* **Encrypted DA.** `PanelDa` is served to five seats in the clear. Encrypting to seats (ML-KEM)
  is a later ADR if seat leakage is measured to matter.

## 9. Number hygiene

This is ADR-0077; ADR-0076 is the last committed on `main`. A concurrent claimant of 0077 renumbers
the later writer, per ADR-0036 Decision 5.
