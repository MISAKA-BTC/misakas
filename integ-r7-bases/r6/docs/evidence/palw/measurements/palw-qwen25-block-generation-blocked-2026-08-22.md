# Qwen2.5-1.5B block generation — STOPPED at the engine/profile correspondence

**Date:** 2026-08-22 · **Verdict: Qwen2.5-1.5B cannot produce a block on this build.** The model
runs and is reproducible; its execution **cannot become a step leg**, so there is no trace root, no
PoW and no block. Stopped here rather than worked around, because the gap is structural.

## The failure, exactly

```
qwen25-convert <dir> --model-id Qwen/Qwen2.5-1.5B --execute
  execute: canonical job 8 prefill / 4 decode, 366184 step leaves
  the canonical job did not execute: the capture could not become a leg:
      NotACanonicalCoordinate { layer: 0, slot: 2, tile: 4 }
```

`slot` there is the table-LOCAL index and the error carries no widths, which is not enough to act
on. `--check-capture` compares the engine's captured row widths against the profile's declared
ones, node by node, at position 0:

```
capture check: 1068 rows at position 0
capture check: 842 disagreement(s)      = 533 width mismatches + 309 rows with no profile node
```

**The engine emits 38 attention-table rows per layer. The Qwen profile declares 27.** Eleven rows
per layer × 28 layers = 308 have no node to land in at all. Of the rows that do land, the widths
diverge from index 2 onward:

| engine idx → slot | op | engine | profile |
|---|---|---|---|
| 2 → 3 | MatMulQuant | 1536 | 256 |
| 3 → 4 | MatMulQuant | 1536 | 256 |
| 4 → 5 | MulElem | 256 | 1536 |
| 7 → 8 | RopeImrope | 256 | 1536 |
| 8 → 9 | RopeImrope | 1536 | 256 |
| 10 → 11 | MatMulQuant | 256 | 12 |
| 13 → 15 | MatMulQuant | 12 | 1536 |
| 18 → 19 | MatMulQuant | 1536 | 8960 |
| 25 → 26 | MulElem | 8960 | 1536 |

The 1536/256 pairs swapping places is the grouped-query boundary (12 query heads at 1536, 2 kv
heads at 256) landing in different positions in the two descriptions; the 8960s are the FFN width
arriving where the profile expects the residual width and vice versa. **These are two different
graphs, not one graph with a width bug.**

## Why this was not caught before

`docs/palw-qwen25-class-phase0.md` Phase 4 reports the real checkpoint running: "the full 28-layer
forward runs, completes, collapses nowhere". That measurement is `measure_depth_health`, which is a
**forward pass**. The step capture — every node output tiled and Merkleised into the commitment an
attempt carries — is a different code path and was never run for this class. A model can run
perfectly and still be unable to commit to what it ran.

This is ADR-0049 **Decision F**, and phase 0's own Gate 0 item **0.2** names it: *"the engine, the
profile, the adjudicator and the inventory are four hand-written descriptions of one computation"*,
with the interim rule *"no worker may commit a step leg for a profile that omits a narrowing the
engine performs."* The Qwen profile omits eleven nodes per layer and misplaces the rest.

**The floor is unaffected**, through the same capture path:
`the_court_convicts_no_leaf_of_an_honest_execution` still sweeps 914/914 leaves. This is specific
to `palw_qwen25_profile`'s node table, not to the capture machinery.

## What was completed, and is not blocked

* **The arithmetic is canonicalized** (the task's item 0), and the split it closed was real:
  `QWEN25_1_5B` declared `rms_eps_q: 1` while `qwen25-convert` hardcoded `eps_q: 1 << 8`, inherited
  from the floor. Two arithmetic specifications under one model id — and the engine norms with the
  artifact's epsilon while the court re-norms with the class's, so an artifact built at 256 under a
  class registered at 1 has **every honest execution convicted**. `misaka-palw-base0::classes` is
  now the single table; the converter looks the shape up and treats `config.json` as something to
  CHECK the checkpoint against. `every_canonical_class_agrees_with_its_own_profile` asserts
  `artifact_shape.eps_q == profile.base0_rms_eps_q` for every entry.
* **A generic class resolver** — `resolve_class_v1(court, class_id, artifact_root, supplied)` —
  table-driven, no per-model branches. The floor resolves from nothing (it is derived); a converted
  class resolves from a supplied artifact and is refused unless BOTH its shape and its inventory
  root match what the chain registered. `the_floor_resolves_from_nothing_and_its_root_is_the_pinned_one`
  proves the registry agrees with the shipped RC pin.
* **The artifact can leave the process**: `encode/decode_artifact_file_v1`, digest-checked on load.
* **The class would be admitted**: `the_admissible_qwen25_class_passes_the_admission_gate` runs the
  four checks `verify_class_admission_v2` runs, against the shipped bundle, and passes —
  `class_id 404af59f…`, 366,184 canonical leaves. The gate also corrected a value: `pwu_per_inference`
  is COUNTED, and the first attempt declaring 1 was told 366,184.

Canonical values, from the real checkpoint at the admissible geometry (tile 64 / n_ctx 125):

| | |
|---|---|
| `execution_class_id` | `404af59f2f16477294e352ed92340dda7928fc3daf034b43a09fac968c3f153b…` |
| `artifact_root` | `c5f2ac618edf9d5cc0c78f5d805b27b334f7c18beedd066385330be780193ebb…` |
| artifact | 1695 MiB, reloads to its own digest |
| arithmetic | `eps_q = 1`, `max_position = 125` |
| forward | alive, 0/28 railed, reproducible |

Note `artifact_root` did **not** move when the epsilon changed from 256 to 1: the inventory covers
operand ROWS, and epsilon is not one. The pair `(class_id, artifact_root)` is what distinguishes
the class — the class id did move — which is why `resolve_class_v1` checks the artifact's shape
field by field and not only its root.

## Reproduction

```
cargo build --release -p misaka-palw-base0 --bin qwen25-convert
# a Qwen2.5-1.5B checkpoint directory: model.safetensors, config.json, tokenizer.json
./target/release/qwen25-convert <dir> --model-id Qwen/Qwen2.5-1.5B --check-capture   # the 842
./target/release/qwen25-convert <dir> --model-id Qwen/Qwen2.5-1.5B --execute         # the refusal
```

## What closing it needs

> **Both were done, in that order.** The narrower fix landed first (`4ea842b0`: the attention table
> projected through `Base0IrGeometryV1`, 842 disagreements → 0). The generator followed on
> 2026-08-26 (`09bd647f`/`df63a916`): the engine's op sequence is compiled from the IR, so the two
> descriptions are one and `--check-capture` can no longer have anything to find.

One of the two descriptions has to be made from the other. ADR-0049 Decision F's answer is a
generator — the profile, the engine's op sequence, the adjudicator's node table and the inventory
all projected from one canonical IR — and until that exists the narrower fix is to rewrite
`qwen25_profile_v1`'s attention table to be what the engine emits, node for node and width for
width, with a test that walks a real capture against it. **Hand-editing it to make one job pass is
not a fix**: the same divergence would still be there for every other geometry, and the court would
convict honest executions wherever the two still disagree.

Whichever is chosen, the check that proves it is the one that found this: run a real capture
through `push_call` and require zero disagreements. `--check-capture` is that check.
