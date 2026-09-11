# ADR-0110 — A context limit is activated from reproducible public vectors, not the maintainer's workstation

* Status: PROPOSED 2026-09-11 on `feat/adr-0103-held-context`, written from the operator's decision
  of the same day (ADR-0103 §10.5 answered by ADR-0109; this is the other half of that answer).
  Consensus-inert: no object, acceptance rule, fence, parameter or fingerprint moves. A fleet takes it
  by an ordinary rebuild.
* Builds on: [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (§10.5: no 2M claim ran; the "done when" was met at devnet widths and at 2M only in the
  generators' tables), [0109](0109-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md) (the
  last prosecution gap before arming), [0097](0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md) and [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) §5 (a
  figure is a generated artifact, and a figure an ADR prints that the generator does not is a bug in
  the document), and ADR-0108 on `feat/adr-0108-extension-envelope` (Decision 4: a receipt is
  evidence of reproduction and never a vote; Decision 6: a ruleset change is described and costed,
  and activated only by a release).
* Supersedes nothing. Amends nothing in consensus.

## 0. The sentence this ADR is

**A context limit is armed only by a release whose evidence anyone can reproduce. The evidence is
a fixed vector — a name, a seed and a geometry, from which the class, the weights and the job are
all derived with no free parameter — and one verifier, `misaka palw verify-context`, which runs the
network's own pipeline over it: produce, commit, seat, court, availability, fit. It prints one
canonical document. Its consensus facts must match byte for byte on every honest machine; its host
facts are reported and never compared. A signed receipt of that document says who reproduced it.
It is evidence, not a vote: no code path reads it, and no number of receipts arms anything.**

## 1. What was missing

ADR-0103 made every term the chain carries per claim constant or logarithmic in the context, and
ADR-0109 closed the last prosecution gap. The regime's evidence at 2M is still the generator's
arithmetic. No 2M claim ran, because the dense row's cache at 2M is 112 GiB and the maintainer's Mac
seats nothing that size (ADR-0103 §10.5). The operator's decision states the consequence both ways.
The code can ship to testnet unarmed. The context is not armed on a result only one workstation
could see.

Arming a width needs five things, and none of them existed:

1. **A job anyone can regenerate bit for bit.** Every fixture that exercises the held rows lives in
   a `#[cfg(test)]` helper, and the drills pick their prompts from a live chain's anchors.
2. **One command that runs the pipeline, not a model of it.** The generator (`palw-model-fit`)
   prices walls from predicates; the drills run a devnet. Neither replays a named job at a named
   width end to end.
3. **A document that separates what must agree from what may differ.** Two honest machines agree on
   every root and disagree on every timing, and a report that mixes them cannot be compared.
4. **A signature that says who ran it without making it a vote** — the thing the operator ruled
   out: a user's "PASS" signature is never consensus truth, and no "three validator attestations"
   activate anything.
5. **CI that runs the small widths on every change**, so that "it worked at 2M once" is not the only
   statement anyone can make.

## 2. Decisions

**Decision 1 — a vector is a name, a seed and a geometry; everything else is derived.**
`PalwContextVectorV1` names the family's held row (the dense A16 graph-v7 row; a hybrid row when its
held drills exist), its geometry (layers, widths, heads, vocabulary, `n_ctx`, tile length), the job's
prefill and decode counts, the prompt-ids form (the Merkle form the held mint mandates) and a seed.
From those, and nothing else:

* the class is the family's profile projected at the geometry, and its id is `H(profile)`;
* the weights are the family's deterministic derivation from the seed (the floor's and the test
  fixtures' own derivation — no file is read);
* the prompt is `prefill` ids drawn from `BLAKE2b-XOF(key = "misaka-palw/context-vector/prompt/v1",
  seed)`, each reduced modulo the vocabulary, and the job's anchor is `BLAKE2b-512(key =
  "misaka-palw/context-vector/anchor/v1", seed)`.

`vector_id = BLAKE2b-512(key = "misaka-palw/context-vector/v1", canonical bytes of the vector)`. The
shipped vectors are named by width (`0110-dense-v7-512`, `…-4k`, `…-32k`, `…-128k`, `…-2m`), and
each name's seed is `BLAKE2b-512(key = "misaka-palw/context-vector/seed/v1", name)`, so nobody
chooses a seed — a vector cannot be re-rolled until it passes. The geometry is the thinnest the held
row admits, not a real model's. What a vector proves is the protocol at the width: the tree depths,
the interval geometry, the court's openings at a deep ladder, the availability units at a large
chunk count. What a real model costs at that width is the generator's table, and Decision 7 says so.

**Decision 2 — the verifier is the network's pipeline, stage by stage, through the seam a node
holds.** `verify-context` builds the family backend exactly as a node's registered-row path does,
and runs these stages in order. A stage that fails still lets the later stages report by name:

| stage | what runs | pass means |
|---|---|---|
| `produce` | the job, executed and retained as the executor retains it (the fold) | a capture, and the answer's ids |
| `commit` | the binding and the claim's roots, derived from the capture | the roots the chain would record, each printed |
| `seat` | every interval opened and verified along the class's route (recompute, or resume from served state — ADR-0103 Decision 2) | every interval `Valid` |
| `court` | the executor's evidence (ADR-0109 Decision 1) at the vector's sampled leaves — the first prefill leaf, an interval's first leaf, the last decode leaf — judged by `palw_one_move_verdict_v1`; then a capture tampered at one of them, verified by a seat and judged the same way | honest leaves `FalseAccusation`; the tampered interval a fault, and its leaf `ExecutorGuilty` |
| `availability` | each held unit (a prompt tile, a state chunk, a step range, a leaf's evidence) answered from the retention and checked by `palw_held_da_check_disclosure_v1` | every answer accepted |
| `fit` | `palw_model_fit_v2` under the held regime at the vector's width, on the named ruleset | every chain wall constant or logarithmic, and admitted |

**Decision 3 — one canonical document, and its id covers only what must agree.**
`PalwContextVerificationV1` is RFC 8785 (JCS) JSON, canonicalised by the one canonicaliser this tree
already has (`misaka_palw_derive::canon_json`). It has five sections:

* `vector` — the vector, and `vector_id`;
* `ruleset` — the network, its `consensus_params_id`, and the fences the fit read;
* `consensus` — every fact the chain would check or a seat would compare: the class id, the artifact
  root, the execution, trace, output, step and checkpoint roots, the leaf and checkpoint counts, the
  interval count and width, each sampled leaf's evidence size and verdict, each availability answer's
  size, the fit's orders and needs;
* `verdicts` — each stage `pass`, `fail (reason)` or `skipped (reason)`. There is no bare overall
  PASS: the document says which stages ran;
* `host` — wall time per stage, peak resident memory, the CPU and operating system, the thread
  count, the build's version and commit. It is reported and never compared.

`document_id = BLAKE2b-512(key = "misaka-palw/context-verification/v1", canonical bytes of the
document without host)`. Two machines that reproduce a vector print the same `document_id`, or one
of them has found a bug, and the two documents are the preimage of the bug report.

**Decision 4 — a receipt is evidence, not a vote.** This restates ADR-0108 Decision 4's rule for
this kind. `--sign-with <key>` writes a `PalwContextReceiptV1`: the `document_id`, the canonical
host section, and an ML-DSA-87 signature by the verifier's key over the receipt's canonical bytes,
under the context `misaka-palw/context-receipt/v1`. That context is a constant of the verifier and is
not added to the bundle's `signature_contexts_root`: the chain verifies nothing signed under it, so
its consensus set has no reason to know it. A receipt cannot be replayed as an attestation, a bond
message or anything else ML-DSA signs in this system, and nothing signed for the chain verifies as a
receipt. **No code path in `kaspad`, `kaspa-consensus`, `kaspa-consensus-core` or the SDK reads a
receipt, and no count of receipts changes any admission, activation, fence, seat, price or share**
(§5 invariant 6). A receipt proves one thing: a named key ran this build over this vector and
reported this `document_id`.

**Decision 5 — CI runs the small widths on every change; the 2M vector is published, not assumed.**
The 512, 4,096 and 32,768-position vectors run in the test suite, and their documents' `consensus`
sections are pinned byte for byte, so a change that moves a root, a count or a verdict at any of those
widths fails by name. The 131,072-position vector runs where the suite's budget allows, and is
otherwise a named external run. The 2M vector is external: its command is published, and so is the
first reproduction's document, with the host and the time it took. Its `consensus` section is then
pinned beside the others. It is pinned because a reproduction exists, not because enough of them
agreed.

**Decision 6 — arming a wider context is a release, and the release cites its evidence.** A ruleset
move that arms `palw_held_context`, or admits a class whose context was unreachable before, is
ADR-0108's tier C: a coordinated release with a fork-id notice, never a manifest and never a count.
Its ADR names, before any height is scheduled:

1. the vectors at the widths the move admits, each with its `vector_id` and the `document_id` the
   release build prints — green in CI up to 32,768 positions, and published above that;
2. ADR-0109's two drills green on the release build;
3. every open prosecution gap at the widths the move admits closed, or named as accepted (as of this
   writing, ADR-0085 Decision 3's close from served intervals against a lying interval, ADR-0109
   §8.4);
4. the fingerprint, identity and schedule the release prints, and which of them move.

**Decision 7 — what a vector does not prove, said in the document.** A vector proves the protocol at
a width with the thinnest model the row admits. It does not prove:

* the real model's resources at that width — the cache, the fetch, the replay time a seat needs.
  Those are `palw-model-fit --preset held`'s table (ADR-0103 §10.2), and the document names it rather
  than restating it;
* anything about a network of many seats, its latency or its bandwidth;
* that a class's weights are what anyone said they were — that is the artifact root's job, not this
  one.

## 3. The document (abridged)

```jsonc
{
  "document": "misaka-palw/context-verification/v1",
  "vector": { "vector": "misaka-palw/context-vector/v1", "name": "0110-dense-v7-4k",
              "family": "a16-graph-v7", "geometry": { "layers": 2, "hidden": 8, "ffn": 8, "heads": 2,
              "kv_heads": 2, "head_dim": 4, "vocab": 64, "n_ctx": 4096, "tile_len": 4 },
              "job": { "prefill": 4092, "decode": 4, "prompt_ids_form": "merkle-v1" },
              "seed": "<128 hex>" },
  "vector_id": "<128 hex>",
  "ruleset": { "network": "devnet", "preset": "held", "consensus_params_id": "<64 hex>" },
  "consensus": { "class_id": "…", "artifact_root": "…", "execution_root": "…", "trace_root": "…",
                 "output_root": "…", "step_merkle_root": "…", "checkpoint_merkle_root": "…",
                 "step_leaf_count": 0, "checkpoint_count": 0, "interval_count": 0, "interval_width": 0,
                 "court": [ { "leaf": 0, "evidence_bytes": 0, "honest": "false-accusation",
                              "tampered": "executor-guilty" } ],
                 "availability": [ { "unit": "state-chunk", "answer_bytes": 0, "accepted": true } ],
                 "fit": [ { "wall": "…", "order": "logarithmic", "need": 0, "limit": 0 } ] },
  "verdicts": { "produce": "pass", "commit": "pass", "seat": "pass", "court": "pass",
                "availability": "pass", "fit": "pass" },
  "document_id": "<128 hex>",
  "host": { "stages_ms": { "produce": 0 }, "peak_rss_bytes": 0, "cpu": "…", "os": "…",
            "threads": 0, "build": "…" }
}
```

## 4. The CLI

```text
misaka palw verify-context --vector <name> | --vector-file <file>
                           [--ruleset devnet-held] [--stages produce,commit,seat,court,availability,fit]
                           [--out <document.json>] [--sign-with <key-file>]
misaka palw verify-context --list                 the shipped vectors, their ids and what each costs to run
misaka palw verify-receipt <receipt.json> [--document <document.json>]
```

`--vector-file` runs a vector that is not shipped, so a stranger can pose a width, and its document
says `shipped: false`. `verify-receipt` checks the signature, recomputes the `document_id` from the
document if one is given, and refuses a receipt whose context is not this ADR's.

## 5. Invariants the tests must hold

```text
1  The generator has no free parameter: a vector's prompt, anchor, weights and class are pure
   functions of its name, seed and geometry, and two calls agree byte for byte.
2  At 512, 4,096 and 32,768 positions every stage passes, and the consensus section equals its pin.
3  The document_id excludes the host section: changing any host field leaves it unchanged, and
   changing any consensus field moves it.
4  A tampered vector is caught at its leaf: the seat's interval is a fault, and the evidence is
   ExecutorGuilty.
5  A receipt round-trips: sign, canonicalise, verify. A changed byte fails; a signature under
   another context fails; an unsigned document verifies as unsigned and says so.
6  No consensus, node or SDK crate reads a receipt or a document (checked by a pin over their
   sources, the way ADR-0108 I-9 is).
```

## 6. Order of work

1. The vector, the generator and the stages, in `misaka-palw-base0` (the seam's families live there).
2. The document, its canonical form and its id; the receipt; the CLI verbs in `misaka-cli`.
3. The pinned small vectors in the suite; the 128K vector's cost measured and placed.
4. The 2M vector's command published; its first document published and pinned.

## 7. What is deliberately not decided

* **The hybrid row's vectors**: the same stages once its held drills exist.
* **Where documents are published.** A document is content-addressed by its `document_id`, so where
  it lives does not matter (ADR-0108 Decision 9's reasoning).
* **The height of any activation**, which is the release's.

## 8. Number hygiene

The operator's draft called this ADR 0105. 0104–0108 were resident on other branches when it was
written (ADR-0109 §7), and ADR-0109 took 0109, so this is 0110. No 0110 was resident on any branch
on 2026-09-11 when it was first committed.
