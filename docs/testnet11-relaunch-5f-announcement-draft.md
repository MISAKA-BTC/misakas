# MISAKA testnet-11, Relaunch 5f — announcement draft

**Status: DRAFT.** Every `<…>` is a value the cut fills in.

**Two rules this draft is held to, because a reviewer caught it breaking both.**

1. **A number is either followed by the command that reproduces it, or it does not appear.** An
   earlier revision opened by claiming every figure had a reproduction command in the document and
   then contained *zero runnable commands* — the strongest sentence in the draft, and false. That is
   the same defect the release itself keeps finding: a claim about coverage with nothing behind it.
2. **Every figure names the court it was measured under.** Numbers from the dissection court
   (ADR-0082) and numbers from the binary court that ships today are different numbers, and an
   earlier revision mixed them — stating a close of one carrier while the branch as it stood
   registered a class whose close was fourteen and unfileable. The genesis card carried that
   distinction and the announcement derived from it dropped it, which is the wrong direction for
   honesty to travel.

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

**3. Certify the class yourself, with the same two commands an operator runs.**
The class that produces these artifacts can be certified on both lanes, and each certification object
fits one carrier — no chunking, so the split-carriage path is not on this row's critical path at all:

    palw-certify drill  --family a16-v5 --lane attempt   # 6 fault vectors, 10 kernels, 76,873 B
    palw-certify drill  --family a16-v5 --lane fp        #                              80,293 B
    palw-certify bind   --artifact <the .palwart> --lane fp
    palw-certify bind   --model-id "Qwen/Qwen2.5-1.5B/graph-v5@512" --lane fp

The last two name the same class, and it is the one the genesis registers. **The certificate's
kernels are the kernels a fault was actually planted in** — the evidence carries the drilled set and
the coverage rule requires the declared set to be a subset of it, so a family cannot claim an
adjudication nobody performed.

**4. Re-run the measurement that says the model can do this at all.**
`palw-model-gate` (dense) and `palw-qwen36-model-gate` ship as binaries. `docs/evidence-qwen36-model-gate/`
carries the model's own answer bytes and the artifacts derived from them.

## The numbers, as measured

**Measured on this build, reproducible from a checkout:**

| | value | reproduce it |
|---|---|---|
| registered context | **512 tokens** | `cargo test -p kaspa-consensus-core --lib -- the_registered_row_names_the_ladder_it_needs --nocapture` |
| decode budget at 512 | 503 max | `n_ctx − chat template (8) − prompt`; 503 is the ceiling, available only to a one-token prompt. The corpus needs 261 and 286, so it fits with room — but a real request costs more than one. |
| the corpus's own cost | cad 66, music 261, scene 286 | `cargo test -p misaka-palw-derive --test corpus_width -- --nocapture` |
| dense class faithfulness | **45 of 57 top-1**, 56/57 top-5, rank corr 0.893 | `qwen25-convert <checkpoint-dir> --a16`, which prints it at conversion time |
| artifacts open in foreign software | 4 agreed, 0 disagreed | `python3 scripts/misaka-palw-artifact-thirdparty.py --require docs/evidence-qwen36-model-gate/artifacts/` |
| the format checker has teeth | five injuries, each refused by name | `python3 scripts/misaka-palw-artifact-conformance.py selftest` |
| a second implementation agrees | — | `python3 scripts/misaka-palw-derive-stranger.py` |

**Publication gate on that last row.** Running the five commands above found the stranger red on the
pre-cut branch — the transformer ids moved when the source tree changed, which the single re-pin at
the freeze closes. **It must be green before this announcement publishes**, because a document that
tells a reader to run something and hands them a failure has spent its credibility on its own first
paragraph. Verified state at the time of writing: corpus width ok, foreign parsers 4 agreed / 0
disagreed, five injuries each refused by name, ladder gate 2/2, stranger RED pending the re-pin.
| every gate this release must pass | one verdict per gate | `bash scripts/ci-gates.sh` |

**Measured under the dissection court (ADR-0082), which this release is held for:**

| | value |
|---|---|
| dispute cost, at EVERY position class | 40,461 bytes — under one carrier, measured per class rather than averaged |
| close size at 512 | 82,719 bytes, one carrier; 82,911 at n_ctx 4,096 — **64 bytes per doubling of context** |

Under the binary court alone those figures do not hold: a dense 512 close is 1,154,673 bytes and
fourteen carriers, which the split-carriage path cannot file. **That is why this release waits for
the dissection court rather than shipping without it**, and it is the honest statement of what the
delay buys.

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

* The **split-carriage court close is open**, at up to 27 chunks on this network. The registered
  class does not need it — its close is one carrier — and a declaration risks a 0.3375 MSK assembly
  deposit against posted collateral, collected on every session ending except the close it pinned.
* The **hybrid tier is not registered**, and the reason is not a margin: its close is three carriers
  at every context width, because it binds a recurrence rather than attention. There is no width at
  which it stops needing the split path.
