# MISAKA testnet-11, Relaunch 5f — announcement draft

**Status: DRAFT.** Every `<…>` is a value the cut fills in. Every number without brackets is
measured and has a command in this document that reproduces it. Nothing here may be softened at
publication time — the wording constraints in §8 of the genesis card exist because each of them
replaced a sentence that was wrong.

---

## What this is

A public test network where **the work that secures a block is a language model's inference**, and
where the thing the model writes is not a score but a file you can open.

A miner runs a real model on a real prompt. The answer it produces is a small declarative
description — a piece of music, a solid, a scene — and the chain turns that description into a
`.mid`, an `.stl` or a `.glb` by a rule everyone runs identically. **The model's answer IS the
artifact's source, canonicalized.** The chain does not carry the file; it carries a derivation, and
anybody can recompute the file from it and get the same bytes.

## What you can check yourself

Three things, none of which require trusting this project's own code:

**1. Recompute an artifact from a derivation, in a different language.**
`scripts/misaka-palw-derive-stranger.py` re-derives the artifact bytes in Python from the
specification, independently of the Rust that produced them. If it disagrees, it says so and refuses
to be used as a verification: *a second implementation that is wrong proves nothing and accuses the
innocent.*

**2. Open the result in software nobody here wrote.**
`scripts/misaka-palw-artifact-thirdparty.py` hands each artifact to the library that ecosystem
actually uses — `mido` for MIDI, `pygltflib` for glTF, `numpy-stl` for STL — and compares a quantity
with MEANING against the description: a solid's enclosed volume, a song's playback duration. On the
demonstration corpus the STL's enclosed volume is **36.0**, and the description's sketch has
shoelace area 12 extruded 3. A library that has never seen this project agrees the solid is the
solid.

**3. Re-run the measurement that says the model can do this at all.**
`palw-model-gate` (dense) and `palw-qwen36-model-gate` ship as binaries. `docs/evidence-qwen36-model-gate/`
carries the model's own answer bytes and the artifacts derived from them.

## The numbers, as measured

| | value | how it was measured |
|---|---|---|
| registered context | **512 tokens** | the corpus's own answers cost 66 / 261 / 286 tokens; at 256 the budget is 247 and two of the three do not fit |
| decode budget at 512 | 503 tokens | `n_ctx` minus the 8-token chat template and prefill |
| dense class faithfulness | **45 of 57 top-1**, 56/57 top-5, rank correlation 0.893 | the conversion's own log, at conversion time |
| dispute cost, any position | 40,461 bytes — under one carrier | every position class: prefill, first decode, tile-aligned, straddling, last |
| close size at 512 | 82,719 bytes, one carrier | 82,911 at n_ctx 4,096 — **64 bytes per doubling of context** |

**45 of 57 is not 57 of 57.** The class is an integer quantization of a float checkpoint and it
does not reproduce the reference exactly; that number is in a log anyone can read and it is stated
here rather than left to be discovered.

**"Thousands of tokens" is exact. "Tens of thousands" is not.** The close is flat except the
prompt-id term to about n_ctx 4,096; beyond that the generated-token ids become a second
context-linear term that this release does not anchor.

## What the evidence does NOT cover

**"The QWEN36 lane produced a grammar-valid description" is what the evidence carries. "Qwen3.6
wrote this file" is a different sentence.** The second tier's lane — its graph, executor, tokenizer
and prompt assembly — has been run end to end and produces artifacts three foreign libraries accept.
It was run carrying a 2-billion-parameter model. The 35-billion-parameter model's own answers have
not been measured, and this release does not claim they have.

Of eight prompts in the gate, five produce artifacts. The three that do not are worth naming because
they are the system working: a bare prompt with no schema returned a JSON *schema* instead of an
instance, another invented a key, and a third wrote a fractional velocity where the format's
discipline is integer. **The grammar refused all three.**

## Joining

**You must wipe.** Not "should". At least four chains currently answer to the name
`misaka-testnet-11` — measured on this fleet: 3,793 genesis mismatches and 735 consensus-parameter
mismatches in six hours, from at least four distinct genesis hashes, one of them a relaunch from
several generations ago that is still running. An old application directory cannot complete a
handshake with this network, and what it prints is a genesis mismatch, which reads like a defect in
the new build rather than the expected consequence of keeping old data.

**Check the genesis hash.** It is the only thing that distinguishes this network from the others
using its name.

    network        misaka-testnet-11
    genesis        <genesis hash>
    fingerprint    <consensus params id>

## What was wrong in the release this replaces

Relaunch 5e ran with known unpatched defects, and the commits that describe them are public. Two
worth naming because they were live on a network people were pointed at:

* the free-prompt worker could not be started by any gateway on any platform — an environment
  allowlist omitted the one variable the worker requires, and a security control that withholds a
  worker's error output meant the message naming it was suppressed;
* a class registered without a bond produced a node that started, followed the chain, and registered
  nothing, because two gates disagreed about whether a bond was required and consensus agreed with
  only one of them.

Both are fixed here. They are listed because a release that says nothing about the one before it is
inviting the reader to assume there was nothing to say.

## Known open

* The **split-carriage court close** is refused at the acceptance layer pending its signature
  verification and digest rules. The registered class does not need it — its close fits one carrier
  — and no second tier can be registered until it lands.
* The **hybrid tier is not registered**, and the reason is not a margin: its close is three carriers
  at every context width, because it binds a recurrence rather than attention. There is no width at
  which it stops needing the split path.
