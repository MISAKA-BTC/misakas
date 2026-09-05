# Private prompts: the data-availability court made real, the Merkle prompt ids, and `PanelDa`

Design record for the 2026-09-05 implementation. The three decisions this implements are already
made — ADR-0062 (the data-availability court, as amended), ADR-0081 Decision 3 (Merkle prompt ids)
and ADR-0077 Decision 16 (`PanelDa`) — and nothing here changes one. What this records is what the
tree actually did against those decisions before this pass, what it does after, and the reasoning
that had to be done to make the verification sound rather than merely present.

## 1. What the tree did before

**The data-availability court could not accept any disclosure a shipped class can make.** The
fold arm for `MaterialDisclosed` checked `palw_da_event_hash_v1(preimage) == opening.event_hash` and
`trace_event_opening_root_v2(claim.trace_chunk_count, opening) == claim.trace_root` — a Merkle tree
over event hashes. No shipped class commits such a tree. Every claim's `trace_root` is
`base0_logits_trace_root_v1` (one keyed hash over every logits row, the flat scheme) or
`tiled_logits_trace_root_v1` (a keyed outer hash over the context, the shape, a Merkle root of
per-row roots, and the generated ids — the tiled scheme). `PALW_ATTEMPT_V2_TRACE_CHUNKS = 1`, so the
opening that would have been required is index 0 of a one-leaf tree, `keyed(LEAF, 0 ‖ event_hash)`,
which equals neither root for any preimage. The fold's own tests passed because the fixture set
`env.attempt.trace_root = trace_event_merkle_root_v2(&hashes)` — a synthetic claim, shaped to agree
with the arithmetic.

Consequence: had the fence been armed, with or without a responder, **every accusation would have
won by silence**, because no producer could have produced an object the fold accepts. ADR-0062's
own security amendment names the harm exactly ("armed with no responder, every accusation succeeds
and every producer is slashable for the price of one bond"); the responder was one precondition,
and the arithmetic was the other, unnamed one. ADR-0062 SA-2's text already says what the object
must be — "the event's own bytes in whatever form the class's `logits_scheme_id` names" — so this
is the decision implemented, not a new one.

**Nothing answered an accusation.** `palw_panel.rs` builds `CourtDisclosed` for the arithmetic
ladder and constructed no `MaterialDisclosed`.

**Merkle prompt ids had checkers and no writers.** The court's opened refutation path exists
(`check_execution_step_refutation_opened_v1`), the tiling module exists, and `validate_palw_v2`
refused the fence by name because every producer and the seat's admission still committed and
recomputed the flat digest, and the close object had no field to carry an opening.

**`PanelDa` had every rule but the transport.** The payload rule, the seat's admission, the
withholding arm and the fences all existed; the acceptance walk passed the fence, the transaction
door read `admissible`. What did not exist: a producer that refrains from broadcasting a mode-2
material, a relay that refuses to forward one, a serve that checks the requester is entitled, and a
submitter that can build a mode-2 commitment.

## 2. The disclosure, as the shipped commitments allow it

An accusation names one **event** `(row, tile)`, packed into the existing `missing_event_index`
field as `row << 8 | tile`: `row` is a decode position (0-based), `tile` a tile index within that
row's logits (always 0 on the flat scheme). At the accusation the fold bounds `row` by the chain
fact it holds — `claim.trace_chunk_count × PALW_FP_TRACE_CHUNK_EVENTS_V3` is an upper bound on
the run's decode rows for every shipped claim — and `tile < 256`.

The disclosure carries the claim's **binding** and the event in the class's own scheme:

| scheme (`shape_profile.logits_scheme_id`) | what is opened | how it is checked |
|---|---|---|
| flat (`flat_logits_scheme_id_v1`, BASE-0) | every row and every id (the flat root opens no row alone) | `check_base0_decode_pin`: rows × ids re-key to `binding.full_logits_trace_root` |
| tiled (`tiled_logits_scheme_id_v1`, the model tiers) | the ids, the row's root and its opening in the rows tree, one tile's lanes and its opening in the row's tile tree | the authentication half of the tiled decode-token check: row opening → rows root → outer root == `full_logits_trace_root`; tile leaf → row root |
| out of range | the binding alone | `row ≥ exact_decode_tokens`, or `tile ≥ tiles(vocab)` — the accusation named an event the committed run does not have, and the binding proves it |

Before any of that, the binding is pinned to the claim exactly as the arithmetic court pins it:
`verify_binding_v1`, `full_logits_trace_root == claim.trace_root`,
`committed_execution_root == claim.execution_root`. So a disclosure is an opening of the claim's
own committed execution, by hash arithmetic, with no model run — SA-2 as written. It is bounded by
the ruleset's `max_close_bytes`, the ceiling a decode-token close already fits under (a tiled close
carries two tiles; a disclosure carries one).

**What the DA court proves, stated plainly.** A refuted accusation proves the producer *holds* the
event it committed — possession, not correctness (the arithmetic court's business) and not that it
served any seat (ADR-0065 D4's abstention remains a licence, not a punishment: a producer that
executed but withheld from its panel is voided, never slashed). A confirmed default proves the
producer could not open a piece of its own commitment inside a window twice the finality depth —
which a producer that never executed cannot do for any `(row, tile)` an accuser chooses. That is
the harm ADR-0062 was written for.

**What a disclosure reveals.** On the tiled scheme the outer root is keyed over the generated ids,
so any disclosure publishes the claim's **output** token ids; on the flat scheme, every row and
every id. The **prompt** is never in a disclosure — only `context_hash` is. This is the
"private unless disputed" sentence the gateway shows, made exact: a data-availability dispute
publishes the answer; an arithmetic dispute at an embedding gather publishes one tile of the prompt
(Merkle ids) rather than all of it (flat ids). The price of forcing that publication is a refuted
accusation's charge, `min(⌈reserved / seats⌉, min_collateral_sompi)` — which the audit's O-10
already flags as low on the shipped registry; the number is the operator's, not this design's.

## 3. The responder

A duty: every claim in `DefaultDisputed` whose producing bond this node holds. The panel opens the
accused event out of the capture it already retains (ADR-0084 D7 keeps it home), through one
backend verb `disclose_trace_event`, signs the object's digest under the bond key, and submits it
as a lifecycle carrier through the same queue and funding the court moves use. It answers only
inside the disclose window and never re-executes anything. There is no automatic accuser: a seat
that received nothing cannot distinguish withholding from transport loss, and an accusation against
an honest producer is a charge to the accuser — accusing is an operator's act (`misaka-cli`).

## 4. Merkle prompt ids, wired

The commitment form is a function of the ruleset, resolved once: `Params::palw_prompt_ids_form_at`.
Everything that writes or checks `prompt_token_ids_hash` takes the form — the job-context writer,
the free-prompt admission predicate the seat and both decoders share, the material codec. The court
gains an appended close variant, `ArithmeticOpened { refutation, operand_openings,
prompt_ids_opening }`, admissible only under the Merkle form and routed to the opened refutation
check; a flat `Arithmetic` close that carries ids is refused by name under the Merkle form. The
fence is **genesis-only**, like `palw_uncertified_weightless`, so no chain ever holds claims under
two forms. `validate_palw_v2`'s refusal — "no writer or checker reads it" — is replaced by that
rule, because the sentence stopped being true.

## 5. `PanelDa`, transported

* A producer never broadcasts a mode-2 material; seats pull it with the signed request that
  already exists.
* The relay refuses to admit or forward a mode-2 material whatever peer sent it — defence in depth
  against a misconfigured producer.
* The serve authorizes a mode-2 request only for a seat of the claim's panel or the challenger of
  an open court session on it, read from chain state; and refuses mode-2 material outright when no
  authorizer is installed.
* The gateway and the submitter build mode-2 commitments on request, only where the node reports
  the fence armed (a new `ChainFacts` field), and show `PALW_FP_PANEL_DA_DISCLOSURE_V1` verbatim.

## 6. Where it is armed

A carded mainnet states `palw_da_court`, `palw_prompt_ids_merkle` and `palw_panel_da` on its base
(`mainnet_card_base_v1`), with the trace format version the bundle carries moved to 4 on that card.
testnet-11 stays as it is — every one of these is a genesis rule there or a re-mint, and its bundle
keeps trace format 3. `PALW_STATE_V2_VERSION` is coupled to the arming by test: the day a shipped
preset arms the court, the version must move (ADR-0062's own rule), and the test says so.
