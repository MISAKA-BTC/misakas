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
