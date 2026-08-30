# Free prompts on a registered class — what is left, and why it is executor-side only

Status: specification. Nothing here is implemented. Written from the code as it stands at
`d181577d`, after an attempt to reach "type a prompt, that inference mines" end to end.

## The finding

The free-prompt lane is complete on the consensus side and unusable on the executor side, and the
missing piece is **not** a consensus change, a new class, or a registration path. It is one seam
in a backend.

What works today, measured rather than assumed:

* `misaka-palw-gateway` takes an OpenAI-style request, runs ONE inference through `palw-worker
  --mode v3-job`, and returns the answer with `cu`, `fp_job_id` and the trace/output/schedule
  roots in-band. A prompt through it returned `cu 8221` over 29 prompt / 128 completion tokens.
* `calculate_l1_tag` has its algo-7 arm; `SUBNETWORK_ID_PALW_FP_COMMITMENT` (0x4a) is validated in
  `tx_validation_in_isolation`; `palw_fp_admission_v3` implements all eight items.
* testnet-11 runs `palw_rc_params` — a `ConsensusV2` network with the free-prompt bundle installed.

Two documents disagree with that code and should be corrected when this lands:
`docs/palw-freeprompt-gateway.md` and `misaka-palw-gateway/src/bin/rail.rs` both say no network
accepts subnetwork 0x4a. One does.

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
