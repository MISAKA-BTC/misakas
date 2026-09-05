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

## 3. The responder, and the accuser

A duty: every claim in `DefaultDisputed` whose producing bond this node holds
(`palw_da_duties_v2`). The panel opens the accused event out of the capture it already retains
(ADR-0084 D7 keeps it home), through one backend verb `PalwBackend::disclose_trace_event` (the
floor and both model tiers implement it), signs the object's keyed digest under the bond key, and
files `MaterialDisclosed` as a lifecycle carrier through the same queue and funding the court moves
use. It answers only inside the disclose window and never re-executes anything.

There is no automatic accuser: a seat that received nothing cannot distinguish withholding from
transport loss, and ADR-0065 D4 (armed on every shipped V2 network) already turns "I never got it"
into a slash-free abstention. Accusing is an operator's act: `misaka palw da-accuse --claim <id>
--row <r> [--tile <t>] --bond <txid:index> --key-file <seed> --out <file>` builds and signs a
`DefaultAccused` (after checking the key is the bond's registered one, so the carrier's fee is not
spent on a signature the chain refuses), and `palw submit-object` files it. The chain's rules — an
Active bond above the floor, a seat of the claim's panel or a bonded challenger, never the
executor, the event inside `trace_chunk_count × 256` rows — are the chain's; the command
re-derives none of them.

## 4. Merkle prompt ids, wired

**The form is a network constant.** `Params::palw_prompt_ids_form_v1()` is
`palw_prompt_ids_form_at(0)`, and `validate_palw_v2` refuses `palw_prompt_ids_merkle` at any
height but genesis: a fence that flipped mid-chain would make the form a function of each job's
anchor height, which no reader of a `prompt_token_ids_hash` holds — a decoder sees bytes, the tx
door holds no DAA score, a worker knows only the network it was started for. The bundle's
`trace_format_version` says the same thing inside `palw_ruleset_id_v2` (3 flat, 4 Merkle —
`PALW_V2_TRACE_FORMAT_VERSION_MERKLE_IDS`), written by the assembly from the fence and refused by
`validate_palw_v2` in either direction alone, so two networks cannot share a ruleset id and hash
their prompts differently.

**One spelling of the comparison.** `prompt_token_ids_match_v1(form, ids, committed)` and
`prompt_token_ids_commitment_v1(form, ids)` are the only two functions that touch the form; every
writer and reader takes it as an argument:

* writers — the FP worker runtime (`fp_worker_prompt_ids_form_v1(network_id)`, derived exactly as
  its court is), the panel's canonical job, the raw lane's `base0_rc_job_v1`, and the three
  backends' `job_for_anchor` (`with_prompt_ids_form`, set by the SDK's lineages beside the ladder
  cap, from the registry's `Params`);
* readers — `palw_fp_prompt_ids_admit_v1` and the seat's wrapper, the three payload decoders, the
  payload's `validate_*` family, the transaction door (`TransactionValidator` holds
  `palw_prompt_ids_form` and `palw_panel_da_admissible`), the extraction walk (which used to pass a
  literal `false` for `PanelDa` — fixed: `palw_panel_da_at(block_daa)`), the worker-result
  rebinding in the gateway (under the form the CHAIN reports, `ChainFacts::prompt_ids_merkle`), the
  backends' carried-prompt checks and the four interval checkers.

**The court.** `PalwCourtVerdictProofV2::ArithmeticOpened { refutation, operand_openings,
prompt_ids_opening }`, appended after `AttnDissection` so every shipped close keeps its borsh
discriminant. It runs the same two pins as `Arithmetic` and the opened checker
(`check_execution_step_refutation_opened_capped_v1`), which verifies the tile against the job
context's root before an id is read. `check_close_speaks_the_networks_prompt_form` refuses, by
name, an `ArithmeticOpened` on a flat network and a whole-id-list `Arithmetic` on a Merkle one
(the arithmetic already cannot be fooled — a flat digest is no Merkle root — the gate is a
readable refusal and one spelling per network); the cost bound charges the opening like the operand
paths. The challenger seat builds the opened form itself: on a Merkle network it takes the ids out
of the refutation and opens the one tile at the disputed step's position when the step is a
prefill step (`call_index == 0`); a decode step reads no prompt id and carries nothing.

**What the court is shown.** 32 ids and a Merkle path — the tile the disputed gather read — not the
conversation. That is the whole difference from the flat form, where a dispute at an embedding
publishes every id of the prompt.

## 5. `PanelDa`, transported

The mode is read off a payload's prefix and nothing else (`palw_fp_privacy_mode_peek_v1`: the job
is the first field of `FPM1`, `FPC1` and `FPA1` alike), so the transport can judge bytes before it
can know whether they are honest and without being given the network's form. Then:

* **never announced** — `broadcast_palw_material` returns before `mark_own_material` for a mode-2
  payload (one place, both the producer's and the panel's broadcasts pass through it);
* **never relayed** — `admit_material` answers `PalwGossipAdmit::Private`: an unasked copy is
  dropped without a digest (so the honest pull's answer is not a `Duplicate` of a stranger's
  refused copy); a solicited copy is handed to this node's inbox and still not relayed;
* **never served to a stranger** — the unsigned pull returns nothing (after the read, so the
  refusal is priced like a served answer); the signed pull with no authorizer installed is
  `NotAReader`; with the panel's authorizer, both lanes (the whole capture and the interval
  opening — a prefill interval's opening carries the prompt's embeddings, which anyone holding the
  artifact inverts) require the requester's bond to be in `PalwChainStateV2::claim_readers_v2`:
  the executor, the bound panel's seats, the challengers of open sessions. The answer envelope
  (`FPA1`, which carries the prompt ids) rides the same lane and the same rule;
* **built on request** — the gateway's `--privacy panel-da` (refused per request where
  `ChainFacts::panel_da_armed` is false, before the inference; the disclosure sentence printed
  verbatim at boot); the commitment builder drops the ids from a mode-2 payload
  (`validate_stateless_v3` would refuse them as `PanelDaPayloadCarriesPrompt`), and the submitter
  stages the worker's ids beside the material (`FpStaging::prompt_token_ids`, refused as
  `PanelDaNeedsPromptIds` when absent) for the executor's node to serve to the claim's readers.
* **reported** — `GetPalwProducerFactsResponse` wire version 6 carries `panel_da_armed` and
  `prompt_ids_merkle`, both fail-closed on an older node.

## 6. Where it is armed

A carded mainnet states `palw_da_court`, `palw_prompt_ids_merkle` and `palw_panel_da` on its base
(`mainnet_card_base_v1`), and the assembly writes trace format 4 for it. testnet-11 and devnet stay
exactly as they are — every one of these is a genesis rule there, and their fingerprints do not
move. `PALW_STATE_V2_VERSION` is coupled to the arming by test
(`arming_the_da_court_on_a_network_with_history_moves_the_state_version`): the day a preset with
history arms the court, the version must move past 20, and the test says so. The DA-court windows
are inside the card's depths (`palw_v2_da_court_lattice_daa`) and, since this pass, inside the
withdrawal-delay interlock (§7).

## 7. Limits, stated

* **Private from the public, not from the panel.** Five seats read the prompt to verify the work
  (ADR-0077 D16's own sentence), and an arithmetic dispute publishes one tile of it; a
  data-availability dispute publishes the answer's ids (§2). Withholding from the panel voids the
  claim and slashes nobody (ADR-0065 D4).
* **Availability is the executor's.** A mode-2 claim's material exists on the executor's node and
  the readers it served; a node that goes dark before its seats pulled is voided at the receipt
  timeout, exactly as a public claim whose material was never delivered.
* **The drill runs flat.** `e2e_drill` (family certification) hashes its own prompt flat by
  construction; its root is about arithmetic capability, not about what a prompt is called, and is
  the same value on either kind of network.
* **The rail derives the form from the result** (whichever of the two forms the worker's ids match
  the job under), because it constructs its own devnet bundle and reads no chain facts before it
  builds.
* **The user-facing surface.** The disclosure sentence is printed by the gateway at boot and
  carried in the ADR; a Studio front end that offers "private" must show it to the person typing.
