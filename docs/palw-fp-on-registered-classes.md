# Free prompts on a registered class — what is left, and why it is executor-side only

Status: **implemented**, and this page is now a record of how it got there rather than a plan.
The header it used to carry — "specification; nothing here is implemented" — was written at
`d181577d` and stayed put through the three landings logged at the bottom of this page; a stale
"not implemented" stops the next person looking, which is the exact failure this document opens by
warning about. What the page still is: the reasoning, in the order it was found, for why the FP
lane runs on a class the chain already registers instead of bringing its own. Read the dated
sections from the bottom for the current shape, and
[palw-freeprompt-gateway.md](palw-freeprompt-gateway.md) for the protocol.

## The finding

The free-prompt lane is complete on the consensus side and unusable on the executor side, and the
missing piece is **not** a consensus change, a new class, or a registration path. It is one seam
in a backend.

What works today, measured rather than assumed:

* `misaka-palw-gateway` takes an OpenAI-style request, runs ONE inference through `palw-worker
  --mode v3-job`, and returns the answer with `cu`, `fp_job_id` and the trace/output/schedule
  roots in-band. A prompt through it returned `cu 8221` over 29 prompt / 128 completion tokens.
  (`palw-worker` and its `v3` arms are gone — ADR-0077 Decision 5. The runtime is a family worker
  and the mode is `v3-serve`; see the last section.)
* `calculate_l1_tag` has its algo-7 arm; `SUBNETWORK_ID_PALW_FP_COMMITMENT` (0x4a) is validated in
  `tx_validation_in_isolation`; `palw_fp_admission_v3` implements all eight items.
* testnet-11 runs `palw_rc_params` — a `ConsensusV2` network with the free-prompt bundle installed.

Two documents disagreed with that code and both have since been corrected:
`docs/palw-freeprompt-gateway.md` and `misaka-palw-gateway/src/bin/rail.rs` said no network accepts
subnetwork 0x4a. One does, and both now say so.

## Why the obvious route is the wrong one

The gateway's worker is the llama.cpp/GGUF family, and its `runtime_class_id` is not a class
testnet-11 registers. The tempting fix is to register it — and it is not available, for a reason
worth stating rather than working around: a post-genesis `ClassRegistered` carries
`admission: Some(PalwClassAdmissionCarriageV2)`, which is *the class's execution graph and its
canonical job*, because "nothing else on a running chain can tell the court what the class
computes". `--palw-register-class` derives the class from a `.palwart` artifact for the same
reason. A class is an adjudicable integer computation, and the FP worker is not one.

So the FP lane should not bring its own class. It should run **on a class the chain already
registers** — `PALW-QWEN25-A16` is a real 1.5 B model, already adjudicable, already sharing 200‰.

That inversion is what makes the remaining work small.

## Why a user's prompt is safe here and forbidden one lane over

`PalwExecutionBackendV1::job_for_anchor` states the attempt lane's rule:

> A producer must not choose its own prompt — a class whose executor picks the input is a class
> where "run the model" and "find an input whose output I like" are the same move.

The free-prompt lane answers that with different machinery, not by relaxing it: the win is not the
output's shape but a quantum ticket against the class's receipt target (`palw_fp_admission_v3`
item 5), the claim is bound to a beacon it cannot have chosen (item 3), the window it may be used
in is fixed (item 4), and the whole thing pays only after certification (item 1). Grinding the
prompt buys nothing, because the prompt does not decide the lottery.

This is why the seam below may hand the backend caller-supplied tokens without reopening the
attack the attempt lane closes. It is also why the seam must be a *separate verb*: an
`execute`-with-arbitrary-prompt reachable from the attempt path would be exactly the hole.

## The seam

`palw_fp_execution_v3` already takes measurements and returns bindings — it does not run models:

```rust
pub struct PalwFpRunFactsV3 {
    pub decode_tokens_executed: u32,
    pub stop_reason: PalwFpStopReasonV3,
    pub full_logits_trace_root: Hash64,
    pub activation_leg_root: Hash64,
    pub checkpoint_leg_root: Hash64,
    pub step_leg_root: Hash64,
}
```

The integer backend measures all four leg roots today — that is what makes its attempt claims
adjudicable and what `bisect_prefix_state` and `disclose` read at a rung. They are simply not
surfaced: `PalwExecutionOutcomeV1` returns `trace_root`, `output_root`, `execution_root`,
`trace_manifest_root` and the opaque `material`.

**Unit FP-R1 — the verb.** Add to `PalwExecutionBackendV1`, defaulted to `Err` so no existing
implementor silently gains it:

```rust
fn execute_free_prompt(
    &self,
    job: &PalwFreePromptJobV3,
    prompt_tokens: &[usize],
) -> Result<(PalwExecutionOutcomeV1, PalwFpRunFactsV3), String>;
```

Two returns rather than one, because the outcome is what a panel and a court already consume and
the facts are what the FP derivation consumes; merging them would make one of the two callers
carry a field it must ignore.

**Unit FP-R2 — the assembly.** `PalwFreePromptCommitmentV3` from the pair, via
`palw_fp_job_context_v3` and `palw_fp_execution_root_v3`. Every rule the court applies is applied
here first, which the module already guarantees; this unit adds no policy.

**Unit FP-R3 — the front end.** A gateway that dispatches to the backend registry by class id
instead of shelling out to `palw-worker`, keeping the existing HTTP shape so MISAKA Studio's
prompt-mining panel needs no change beyond a URL. The panel already reads
`class_id` from `/health` and reports whether the chain registers it; on a registered class that
line turns from "cannot be told" into the class's name, which is the whole visible difference.

**Unit FP-R4 — submission.** `misaka-palw-fp-rail` builds and signs the 0x4a transaction and
deliberately stops. Its stated reason is stale (above). Submitting needs a funded UTXO and the
executor bond, both of which a producing node already holds.

## What this still does not buy

A block does not follow a prompt. `palw_fp_admission_v3` item 1 admits a receipt only for a claim
that is **Final**, and certification runs bind → receipt → challenge → court. On the RC bundle's
windows that is chain-time measured in windows, not seconds, and shortening it is a decision about
fraud-proof safety, not a UX tuning knob.

Any interface built on this lane has to show that honestly: the answer is immediate, the mining
right is not. MISAKA Studio's panel states the current stage by name for exactly this reason, and
should keep doing so after FP-R4 — `submitted`, `bound`, `certified`, `spent` — rather than
collapsing them into a word like "mining" that would be true only at the end.


## The A16 finding, which changes the shape of the remaining work

FP-R1 was implemented on BASE-0 and works. Extending it to `PALW-QWEN25-A16` — the class that can
actually hold a conversation — ran into something that is not an engineering gap:

**The class registers a checkpoint state map that cannot describe its own KV cache.**

`integer_kv_state_geometry_v1` derives `row_bytes = attn_kv_heads × attn_head_dim`, one byte per
element. That is exact for BASE-0, whose cache is `Vec<Vec<Vec<i8>>>`. `A16Cache` is
`Vec<Vec<Vec<i32>>>`, and `palw_qwen25_profile` declares
`state_chunk_map_id: integer_kv_state_chunk_map_id_v1()` anyway.

What makes it worth writing down rather than fixing in passing is how it fails. `state_chunk_bytes`
guards by comparing the engine's row LENGTH to the map's `row_bytes`, and for this class those are
the same number — 256 elements, 256 declared bytes — so the guard passes and every value outside
`i8` is truncated. The result is a committed checkpoint that opens to a state the producer never
had: worse than no checkpoint leg, because the producer has signed for it.
`a16_kv_state_does_not_fit_the_one_byte_map_its_class_declares` measures the state rather than
arguing from the type, and the values do exceed a byte.

The consequence is consensus-visible. A sound checkpoint leg for this family needs either a
4-byte state map or a narrowed cache; `state_chunk_map_id` is part of `PalwShapeProfileV3`, the
shape profile id is the class id, so either fix REGISTERS A DIFFERENT CLASS. It cannot be shipped
as a patch to the class testnet-11 already carries.

So the remaining work for "a meaningful answer that is also work" is:

1. this decision — narrow the cache, or register a class whose map has 4-byte elements;
2. the checkpoint capture against whichever map that is (`push_chunks` + `next_geometry`);
3. `disclose` and `bisect_prefix_state`, without which `supports_court` must stay false.

Step 1 is not code. It is the one that has to be made first, and by whoever owns the class set.


## Checked exhaustively: no registered class can carry a meaningful free-prompt claim

The A16 finding above is not one class's accident. testnet-11 registers three, and the property
the free-prompt lane needs — runs a language model AND is adjudicable — belongs to none of them:

| class | language model | `state_chunk_map_id` | `supports_court` |
|---|---|---|---|
| `PALW-BASE-0` | no (integer floor, no tokenizer) | v1 (`i8`), correct for its `i8` cache | **true** |
| `PALW-QWEN25-A16` | yes | v1 (`i8`) declared over an `i32` cache | false |
| `QWEN36` | yes (34 GiB) | `Hash64::default()` — the unregistered sentinel | false |

BASE-0 can make an adjudicable free-prompt claim today; FP-R1's round-trip test proves it. Its
answers are token ids from a model that is not a model, which is the honest reason a person would
not call that "mining with a prompt".

QWEN36 does not even declare a map: the sentinel is the honest value while no layout exists, and
`verify_binding` refuses a binding that files one the class does not register. So it is in the
same position as A16 and one step further back.

**That is the whole remaining distance.** Not a bug to fix in an executor: the chain does not
currently register a class that is both a language model and disputable. Closing it means
registering one — with a state map that describes its cache (`integer_kv_state_chunk_map_id_v2`
exists for that), the checkpoint capture against it, and `disclose`/`bisect_prefix_state` so
`supports_court` can be true without lying. Everything up to that point is landed and tested.


## The second A16 defect: its registered graph omits a narrowing

Implementing the capture found a defect the map problem was hiding, and this one is smaller and
sharper. ADR-0049 Decision F:

> No worker may commit a step leg for a class whose profile does not name every narrowing the
> engine performs.

`Base0Engine` exposes `plan()` and `base0_check_graph_v1` enforces exactly that, per token, before
the first leaf. **`A16Engine` has no plan and there is no A16 counterpart**, so nothing checks it
for this family — and measuring shows it does not hold:

| table | profile declares | engine records |
|---|---|---|
| pre | 1 (`EmbedLookup`) | 2 — the gather, **and the requant that lifts it onto the A16 stream** |
| per-layer | 27 | 27 |
| post | 3 | 3 |

The per-layer and post tables agree exactly. The pre table is short by one node, and the missing
one is `a16_requant(&embed_row, &tile(self.embed_lift, d))` — a requant, which is the kind of
operation Decision F exists for. Decision F also states the consequence: a producer that ran anyway
"would commit to arithmetic the court recomputes differently and be convicted for performing it
correctly".

`Qwen25A16Backend::execute_free_prompt` therefore checks the correspondence and refuses, naming the
node. It does not drop the undeclared row: dropping would be guessing that the omission was
deliberate, and the guess is unfalsifiable from here.

So the corrected A16 class needs TWO changes to its profile, both of which move the shape profile
id and therefore register a different class:

1. `state_chunk_map_id: integer_kv_state_chunk_map_id_v2()` — the width its cache actually has;
2. the pre table naming the embed-lift requant.

Neither is speculative now: the first is measured against the cache's element type, the second
against the engine's own trace. Everything else on the executor side is built and tested.


## The blocker that outranks all of the above: no registered class can earn a draw

Walking the path end to end — a real execution, a real commitment, the real extraction the virtual
processor calls — stops one step before a claim, on arithmetic that has nothing to do with any of
the defects above:

    cu = prompt·prefill_weight + decode·decode_weight  =  prompt·1 + decode·64
    BASE-0      n_ctx 12  →  best cu = 1 + 11·64 =  705
    QWEN25-A16  n_ctx 16  →  best cu = 1 + 15·64 =  961
    quantum_cu                                     = 1000

`fp_quanta_v3` floors, so the widest job either class can hold earns ZERO quanta and the extraction
refuses it with "job earns no quanta" — correctly, because a claim that draws nothing certifies
nothing the chain can act on.

Neither number is wrong by itself. The context ceilings are the court's: 12 and 16 are what the
carrier's worst close allows, and `palw_qwen25_profile` documents the reasoning ("n_ctx 16 is the
widest context whose worst close stays inside the carrier"). The quantum is the bundle's, sized so
an ordinary chat job earns a handful of draws — for a chat job of 100 prompt and 256 decode
tokens, which is 40× more context than any registered class has.

**The two were sized against different pictures of the same lane**, and together they leave no job
that both fits a registered class and earns a ticket. That is why nothing has ever mined here, and
it is upstream of the legs capture, the state map and the graph correspondence: fixing all three
still leaves every free-prompt job worth zero.

Closing it is a consensus decision, and every option moves an id:

* a smaller `quantum_cu` (or lighter weights) — the bundle's, so the ruleset id;
* wider class contexts — the profile's, so the class id, and the court's carrier ceiling is what
  set them, so it is a court-capacity question rather than a free parameter.

`a_callers_prompt_runs_and_no_registered_class_can_earn_a_draw_with_it` pins the arithmetic and the
refusal, so a change to either number has to face this test.


## Why sink adoption is not shown in-harness: DAA does not advance with block count

Measured, not assumed. A `TestConsensus` V2 chain extended by 2,000 sequential blocks reaches a
sink DAA of **63** — `build_block_template_row`'s blocks raise the GHOSTDAG blue score every block
but the DAA score barely moves, because DAA is `block_daa_window(ghostdag).daa_score` and a single
linear chain of same-timestamp blocks does not grow the DAA window the way a real network's mergeset
width does. 2,000 blocks took 264 s.

A free-prompt claim's draw slot is `final_daa + receipt_maturity` — and `final_daa` is itself a
challenge window (1,200) past its license. So the beacon a receipt block needs sits ~1,600 DAA
above the sink, which at ~30 DAA per 2,000 blocks is on the order of a hundred thousand blocks and
hours of test time. That is why `a_receipt_carriage_passes_the_header_signature_gate` proves the
header gate and names sink adoption as uncovered rather than driving it: the harness cannot cheaply
manufacture the DAA a real network accrues over the certification windows, and faking the DAA
directly would be testing a chain state no real chain reaches.

The admission logic that decides sink adoption is proven where it is pure and total
(`check_palw_receipt_spend_admission_full_v3` over a real certified claim, in
`misaka_palw_base0::backend::end_to_end_tests`). What is not shown in a test is the wait itself —
which is the same wait a real network takes in wall-clock, and the reason the goal's final step is
operational rather than a matter of more code.


## Resolved: the quantum is lowered to what registered classes actually reach

The pricing blocker above — no registered class earning a single draw at `QUANTUM_CU = 1,000` — is
closed. The quantum is now **100**, and `PWU_PER_QUANTUM` moved with it (100 → 10) so that a given
CU total contributes exactly the chain weight it did before: `weight = ⌊cu/quantum⌋ · pwu_per_quantum`,
and `10/100 == 100/1000`. Only the granularity changed; the economics did not, and the receipt
lane was not re-weighted against the attempt lane.

The consequence, measured:

| class | n_ctx | widest CU | quanta at 1,000 | quanta at 100 |
|---|---|---|---|---|
| BASE-0 | 12 | 705 | 0 | 7 |
| QWEN25-A16 | 16 | 961 | 0 | 9 |

`the_pricing_is_reachable_on_registered_classes` pins that both registered classes now earn ≥ 1
quantum at the widest job their context holds, and `a_callers_prompt_on_a_registered_class_opens_a_
claim_at_the_shipped_quantum` drives the whole chain path — execute, commit, extract — on the
SHIPPED bundle rather than a rebuilt one, and the claim opens.

This is still a consensus value: `palw_fp_devnet_bundle_v3` is what testnet-11's own params are
built from, so lowering `QUANTUM_CU` moves that network's ruleset id. A network carrying the old
value and one carrying the new are different networks and will not share a chain — which is the
regenesis (or a fresh devnet) the change implies, and the one operational step this cannot perform
for itself. What is no longer outstanding is the DECISION: the value is chosen, measured against
the classes that exist, and it holds weight-per-CU constant so it is not also an economic change
smuggled in beside a pricing one.

## 2026-08-31 — the lane reaches a running network (Relaunch 4)

That operational step happened without this branch having to perform it: testnet-11's Relaunch 4
(fingerprint `5ccdd684…`, genesis `8d2002cc…`) merged this branch — the quantum at 100, the
graph-v2 catalog row, the receipt producer — and its operator doc records the reason in this
document's own words ("at 1,000 no registered class could reach a single quantum"). What remained
between the code and a mined receipt block was measured and closed today, in the order a model
gate should run:

1. **Cross-device agreement, before any registration.** The same prompt (`"What is 2+2?"`, 7
   tokens, decode 9) on the real 1.7 GiB artifact, on this repo's arm64 dev machine and the
   x86_64 fleet host: identical output ids, all four leg roots, execution root, CU (583),
   quanta (5) and claim id. The integer family's determinism claim, held across architectures on
   the real class — not the unit-test geometry.

2. **The gateway had no worker that could commit.** Its only v3-job implementor was the pinned
   llama.cpp worker, whose v3 path documents its own gap — no step leg, null execution root —
   and `FreePromptCommitted` refuses exactly that value (`UnadjudicableCommitment`, the C3
   fail-closed door). So the browser pipeline existed end to end and could never produce an
   admissible commitment. `palw-a16-fp-worker` is the A16 backend behind the same two-mode
   contract; the schedule root is derived exactly as `palw_fp_commitment_v3` derives it, so the
   gateway's `to_commitment` and the canonical assembly agree byte for byte.

3. **Offline, against the live identity.** Gateway + worker under seat-0's real bond, the
   Relaunch 4 network domain (genesis-bound, ADR-0042) and a fresh anchor from the synced chain:
   a browser-shaped chat request returned an answer and a commitment that passes every stateless
   `FreePromptCommitted` condition — non-null execution root included — at 2 quanta.

Two honest limitations, stated rather than hidden: the registered class's `n_ctx` is 16, and the
gateway's plain-marker template costs ~9 of those tokens, so a browser prompt today is a few
tokens each way — the pipeline proof, not the product width; a wider A16 row is a later
registration through this same gate. And the artifact's `tokenizer_commitment` is zero (it
predates the converter learning to stamp one), which nothing on the chain checks today; the
job's `tokenizer_id` carries the same zero consistently. Re-converting with the commitment moves
the artifact digest and is a follow-up, not a blocker.

## 2026-09-02 — certification is a consensus object (ADR-0075), and both model tiers drill the lane

Two of the findings above are now closed, and one premise has moved:

* **The A16 refusal was the legacy class, not the graph.** `a16_execute_for_attempt_v1`'s
  Decision-F probe fires only for a DIRECT caller that passes no plan (`Qwen25A16Backend::new`
  compiled none until ADR-0082's fix E; both constructors compile the plan now) on the
  v1 profile whose `pre` table declares one node against the two the engine records. The
  registered class on testnet-11 since Relaunch 5 is `Qwen/Qwen2.5-1.5B/graph-v2`
  (`qwen25_a16_class_id_v2`), served through `from_registered_profile`, and `execute_free_prompt`
  runs on it — the worker in the section above already does. What refused an A16 free-prompt
  claim on 5d was the transition: `FreePromptLaneUncertified`, because the genesis
  free-prompt-certified set held the floor alone.
* **QWEN36 has the path now.** `Qwen36Backend::execute_free_prompt` commits the same captured
  step leg the attempt lane commits, priced by its leaf count; `refutation_with_prompt` carries
  the caller's prompt into the prover (the split A16 made), so a court can try a Qwen3.6
  free-prompt claim. The fixture graph drills its free-prompt lane (`rc_free_prompt_evidence_v1`),
  as does the A16 fixture.
* **The certified sets are on the chain.** ADR-0075 (`docs/adr/0075-…`) adds two lifecycle
  objects: `FamilyCertified` carries a drill's evidence and the transition grades it with the
  shipped court; `ClassLaneCertified` binds a registered class to a lane by its own profile hash
  and kernel coverage — free-prompt lane: its commitments are admitted; attempt lane: a
  weightless class is seated at the floor share. The genesis free-prompt set is derived by the
  same coverage rule and, on 5e, names the floor, the A16 graph-v2 class and the QWEN36 graph-v3
  class, so the real models take free-prompt claims from genesis; any later model takes the
  on-chain route with `palw-certify drill|bind` and `misaka palw submit-object`.

Still true: the seat replays from token ids and needs no tokenizer, and the artifact's
`tokenizer_commitment` remains zero and unchecked. New: `palw-qwen36-fp-worker` implements the
gateway's two-mode contract for the hybrid tier (`MISAKA_PALW_ARTIFACT` = the converted
`.palwq36`, `MISAKA_PALW_GGUF` = the checkpoint whose header carries the tokenizer,
`MISAKA_PALW_NETWORK_ID`, optional `MISAKA_PALW_MODEL_ID` for another graph-v3 row), so the
gateway can point `--worker` at it and a browser prompt reaches a Qwen3.6 free-prompt claim.

## 2026-09-03 — one runtime, three modes: `v3-serve` (ADR-0077 Decision 1)

The "two-mode contract" named twice above is a three-mode contract now, and the third mode is the
one an operator actually runs.

```text
  --mode v3-manifest   the identity, as one JSON line              (map, print, exit)
  --mode v3-job        one framed request in, one result out       (map, run, exit)
  --mode v3-serve      the manifest, then a resident request loop  (map ONCE, then jobs)
```

`v3-job` mapped the artifact inside every job — about eight minutes per REQUEST on the hybrid
tier, because a 33 GiB artifact is opened, digested and validated before a single token is
decoded. `v3-serve` pays that once: the gateway spawns the worker once, reads its manifest once,
and every later job travels the same framed `PalwFpWorkerRequestV3` / `PalwFpWorkerResultV3` over
the persistent stream, one generation at a time (a single engine, a single KV cache — which is why
the whole runtime sits behind one mutex).

Three things did not change, and each is the answer to a way this could have gone wrong:

* **A job's roots through `v3-serve` are byte-identical to the same job's roots through `v3-job`**
  (invariant W6). Residency is a cost decision; it is not allowed to be a semantics decision.
* **The served width is the CLASS's registered `n_ctx`**, read from the catalog row and never from
  the artifact's rotary span. A runtime that answered wider than the court admits would be the
  two-products split ADR-0077 R0 exists to close.
* **Every job is captured.** `misaka-palw-serve` is retired and the two un-captured chat drivers
  (`base0-chat`, `qwen36-chat`) are deleted, so `court_capable: false` is no longer a state a
  runtime in this tree can be in.

The artifact is re-verified rather than assumed: it is opened read-only, digested at map time, and
re-digested whenever the file's device, inode or size changes (SA-6). An I/O fault on a mapped page
is a `JobFailed`, never a crash of the gateway or the node.
