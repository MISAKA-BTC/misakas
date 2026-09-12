# ADR-0110 — A context limit is activated from reproducible public vectors, not the maintainer's workstation

* Status: PROPOSED 2026-09-11 on `feat/adr-0103-held-context`, written from the operator's decision
  of the same day (ADR-0103 §10.5 answered by ADR-0111; this is the other half of that answer).
  **Decisions 1–5 IMPLEMENTED the same day (§9)**; the 512, 4,096 and 32,768-position vectors pass
  every stage and are pinned. Consensus-inert: no object, acceptance rule, fence, parameter or
  fingerprint moves. A fleet takes it by an ordinary rebuild.
* Builds on: [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (§10.5: no 2M claim ran; the "done when" was met at devnet widths and at 2M only in the
  generators' tables), [0111](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md) (the
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
ADR-0111 closed the last prosecution gap. The regime's evidence at 2M is still the generator's
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
* the job's anchor is `BLAKE2b-512(key = "misaka-palw/context-vector/anchor/v1", seed)`, and the
  prompt is the family's own canonical prompt for that anchor (`base0_rc_job_v1`'s keyed stream,
  each id reduced modulo the vocabulary). It is committed under the Merkle form and carried by a
  free-prompt job, the executor's own entrance, whose every other field is fixed or derived from
  the seed. No generator is new: a vector reuses the derivations a node already runs.

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
| `court` | the executor's evidence (ADR-0111 Decision 1) at the vector's sampled leaves — the first prefill leaf, an interval's first leaf, the last decode leaf — judged by `palw_one_move_verdict_v1`; then a capture tampered at one of them, verified by a seat and judged the same way | honest leaves `FalseAccusation`; the tampered interval a fault, and its leaf `ExecutorGuilty` |
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
The 512-position vector runs in the default test suite, and the 4,096 and 32,768-position vectors run
in the release-mode vector job. Each document's `document_id` is pinned, so a change that moves a
root, a count, a size or a verdict at any of those widths fails by name. The 131,072-position and 2M
vectors are external runs. The command is published, and so is the first reproduction's document,
with the host and the time it took. Its id is then pinned beside the others. It is pinned because a
reproduction exists, not because enough of them agreed. The measured cost of each width is §9's, and
the 2M vector's is not small (§9.3).

**Decision 6 — arming a wider context is a release, and the release cites its evidence.** A ruleset
move that arms `palw_held_context`, or admits a class whose context was unreachable before, is
ADR-0108's tier C: a coordinated release with a fork-id notice, never a manifest and never a count.
Its ADR names, before any height is scheduled:

1. the vectors at the widths the move admits, each with its `vector_id` and the `document_id` the
   release build prints — green in CI up to 32,768 positions, and published above that;
2. ADR-0111's two drills green on the release build;
3. every open prosecution gap at the widths the move admits closed, or named as accepted (the last
   one found, ADR-0085 Decision 3's close from served intervals against a lying interval, was
   closed the day it was found — ADR-0111 §8.5);
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

The operator's draft called this ADR 0105. 0104–0109 were resident elsewhere when it was written
(ADR-0111 §7): 0108 and 0109 on `origin/main`, the extension envelope and the bridge liveness. No 0110
was resident on any branch on 2026-09-11 when it was first committed, and it keeps the number,
because its vectors are named by it (`0110-dense-v7-…`) and every seed and pin is derived from those
names. The leaf-demand ADR it builds on, written before it as 0109, is ADR-0111.

## 9. Implementation record (2026-09-11)

Built on `feat/adr-0103-held-context` (`528d1bbb` and its successors), and merged into the unarmed
testnet-11 integration branch `integ/t11-adr0103-unarmed`. Nothing consensus-side moved.

### 9.1 Where each Decision lives

| Decision | where | what pins it |
|---|---|---|
| **1** the vector | `misaka_palw_base0::context_vector` — `PalwContextVectorV1`, the shipped five (`palw_context_vectors_v1`), the seed `palw_context_vector_seed_v1(name)`, the anchor and the job fields keyed off the seed, the class from `qwen25_a16_profile_v7` at `palw_context_vector_geometry_v1`, the weights from `Base0ArtifactV1::derive_deterministic` | `a_vector_is_a_pure_function_of_its_name_seed_and_geometry` |
| **2** the stages | `palw_verify_context_vector_v1` over the registered-row `Qwen25A16Backend`, judged under `PalwContextRulesetV1::devnet_held_v1` (the `--palw-held-context-devnet` mint) | the pinned vectors below |
| **3** the document | `PalwContextFindingsV1::{agreed_json_v1, document_id, document_json_v1}`; `palw_canonical_json_v1`, checked against `misaka_palw_derive::canon_json` on every write | `the_canonical_form_sorts_and_escapes`; `the_document_id_excludes_the_host_and_moves_with_any_consensus_fact` |
| **4** the receipt | `misaka palw verify-context --sign-with` and `misaka palw verify-receipt` (`misaka-cli/src/palw_verify_context.rs`), ML-DSA-87 under `misaka-palw/context-receipt/v1` | `a_receipt_round_trips_and_refuses_a_changed_byte_or_another_context`; `no_consensus_node_or_sdk_crate_reads_a_receipt` |
| **5** the pins | `the_512_vector_passes_every_stage_and_is_pinned` (default suite); `the_4k_…` and `the_32k_…` behind `--ignored` (`cargo test --release -p misaka-palw-base0 --lib -- --ignored context_vector`) | themselves; the 512 id is the same from a debug and a release build, and on the feature and the integration branch |

### 9.2 What the three widths measured

The dense graph-v7 row at the thinnest geometry, on one M-series host that was sharing its CPU
(load average 20–28) — so the host columns are an upper bound, and they are the columns nobody
compares:

| | 512 | 4,096 | 32,768 | order |
|---|---|---|---|---|
| step leaves | 51,180 | 409,580 | 3,276,780 | linear (held by the executor) |
| capture retained | 81 KB | 610 KB | 4.84 MB | linear (held by the executor) |
| a leaf's evidence (the one-move object) | 6,931 B | 7,315 B | 7,699 B | **logarithmic** — 128 B a doubling |
| a prompt tile answer | 457 B | 649 B | 841 B | **logarithmic** |
| a state chunk answer | 999 B | 1,191 B | 1,383 B | **logarithmic** |
| the widest step range answer | 66,129 B | 66,321 B | 66,513 B | **logarithmic** |
| an interval opening (the widest of four) | 6,077 B | 7,741 B | 19,133 B | linear in the interval's blocks, inside the lane's cap by the width rule |
| produce | 0.57 s | 1.9 s | 42.9 s | host |
| seat (four intervals, recompute route) | 1.8 s | 5.3 s | 217 s | host |
| court (three honest leaves and the drill) | 3.6 s | 9.0 s | 254 s | host |
| availability (five answers) | 0.4 s | 1.3 s | 87 s | host |
| peak resident | 47 MB | 343 MB | 1.64 GB | host |

Every stage passed at every width. **Everything the chain would carry per claim grew by a constant
number of bytes a doubling**, which is ADR-0103's R-held measured end to end rather than read off
a generator. Everything that grew with the context is held by the executor or the seat.

### 9.3 What the widths found

* **The seat's recompute is super-linear in the context on this engine.** From 4,096 to 32,768
  positions (8×) the seat stage took 41× longer and availability 67×. The recompute replays the
  prefix, and the attention over it is quadratic, even at the thinnest geometry. ADR-0103 Decision
  2's route rule prices a recompute linearly (`n_ctx × replay_ms_per_position`), so at wide contexts
  it is optimistic about when `Recompute` fits the window. A release that arms a wide context must
  price the seat with a measured curve, or route it to `Resume`. This is recorded against ADR-0103
  Decision 2; nothing here changes it.
* **The 2M vector is not an evening's run.** Extrapolated from 32,768 at the measured exponents
  (n^1.5 to n^2), the thin row at 2M costs 12–28 days of one process on this host. "External" means
  a large machine for days, or engine work first: a prefill that parallelises across positions, and a
  seat that resumes rather than recomputes. The 131,072-position vector extrapolates to about three
  hours and is the next external run.

### 9.4 What remains

* ~~The release-mode vector job in CI.~~ **Built 2026-09-11.** The `Context vectors (release)` job
  in `.github/workflows/ci.yaml` runs the `context-vectors` gate of `scripts/ci-gates.sh`
  (`cargo test --release -p misaka-palw-base0 --lib -- --ignored context_vector`). The gate names
  both vectors in its evidence and pins the count at two, so a third vector added under that
  filter turns it red instead of lengthening CI. `workflow-parity` holds the two spellings equal.
* ~~ADR-0103 Decision 2's route rule against the measured curve.~~ **Amended 2026-09-11** (ADR-0103
  §10.6): a replay now pays for its history. On this row the 4,096 and 32,768-position vectors take
  the Resume route. They were re-pinned for that alone: `9c5a772c…` and `848d9352…`, and on a less
  loaded host the 32,768 run took 398 s. The 512-position pin did not move.
* The first external documents for 128K and 2M, and their pins. **The 2M vector cannot run on
  this tree**: ADR-0103 §10.7 found that the A16 tier's attention ops refuse a history longer
  than 2^18. The widest A16 vector anything here can produce is 262,144 positions.

### 9.5 How the 2M vector runs (2026-09-11)

The operator asked how the 2M vector should be run. Answering meant running the widths below it
and reading where the time went. Four findings, in the order they bind:

1. **The wall.** No A16-family class executes past 262,143 positions (ADR-0103 §10.7). The 2M
   vector is therefore refused before produce, by name (`palw_context_vector_blocked_v1`), and
   `--list` says so. `0110-dense-v7-256k` (262,144 positions) was added as the widest vector the
   tier can run. Running 2M needs a ruleset decision first: a separate history bound for the
   attention ops, with its exactness argued (it holds to `2^26` rows) and its Q24 precision
   answered (about three bits a probability at `2^21`). A faster machine does not help.
2. **The pool's own overhead was the system time.** Sampled on the 32,768-position vector, 46% of
   thread time was `swtch_pri` and 34% `__psynch_cvwait`: rayon's workers yielding and waiting.
   Two things kept waking them. The fused attention site tiled one registered triple over
   `heads × history` entries twice per call and materialised four history-long rows. The kernels
   also went to the pool on a channel count alone, even for the thin row's 512-MAC unembedding.
   `kernels::a16_attn_fused_uniform_fast` reads each triple once and reuses its scratch, and
   `parallel_worth_it` / `FUSED_CHUNK_WORK` send only real work to the pool. Both are
   bit-identical by construction and pinned against the catalog. The 4,096-position vector went
   from 18.9 s (83 s of system time) to 10.9 s (0.14 s); the 32,768 one from 398 s to 274 s
   (1,312 s of system time to 240 s), on one host at like load.
3. **One claim's questions each walked the job from row zero.** The seat's interval starts, the
   executor's opening anchors, a leaf's evidence and the held units all ask for positions of ONE
   job. `fp_recompute::base0_fp_a16_seat_state_v1` keeps the dense tier's walk between them: an
   attention cache only appends, so a later question continues it and an earlier one reads its
   first rows. `a_resumed_walk_is_the_walk_from_zero` holds it equal to the walk from zero in every
   order.
4. **The court's tampered half re-executes the job dense**, about 700 bytes a leaf. That is 9 GB
   at 128K. At 256K (26.2 M leaves, past `PALW_CONTEXT_TAMPER_MAX_LEAVES_V1`) the stage says so
   and is skipped by name. The honest half still runs at every width. A tamper that re-folds
   instead of re-executing is the next engine change the widest vectors need, and it is not
   built.

**The runs, on this host** (a 12-core M-series, 24 GB, shared with a build), after the three
changes:

| | 4,096 | 32,768 | 131,072 |
|---|---|---|---|
| produce | 1.0 s | 33 s | 312 s |
| seat (four intervals, the Resume route) | 2.8 s | 89 s | 819 s |
| court (three honest leaves and the drill) | 5.9 s | 139 s | 1,402 s |
| availability (five answers) | 0.4 s | 12 s | 140 s |
| whole run | 10.9 s | 274 s | 2,682 s (44.7 min) |
| peak resident | 373 MB | 2.35 GB | 7.5 GB |
| a leaf's evidence | 7,315 B | 7,699 B | 8,019 B |
| `document_id` | `9c5a772c…` | `848d9352…` | `7bc3f88b…` |

The 131,072-position vector's first reproduction is this run. Its id is pinned in
`misaka-palw-base0/tests/context_vector_external.rs` — its own test target, so the release-mode
job's filter (which pins its count at two) cannot see it — and it runs by hand:
`cargo test --release -p misaka-palw-base0 --test context_vector_external -- --ignored`. Every
chain-side term still reads a constant number of bytes a doubling: the evidence grew 320 bytes
over the two doublings from 32,768, the answers likewise, and the roots, counts and verdicts are
the table's. The 262,144-position vector is the next external run; the 2M one waits on §10.7 of
ADR-0103.

### 9.6 Re-pinned for ADR-0119 (2026-09-12)

ADR-0119 walks a held class at the regime's `2^40` ladder instead of the held devnet's `2^26`, and
every vector runs a held class. So each document's fit reports 40 levels on its ladder row, and
its widest close, priced at that depth, grows. The 512 vector's close grows by 5,376 bytes, and
the three wider ones' by 896 (one path, fourteen levels deeper). Diffed field by field against
the documents the previous pins were taken from, nothing else moved: every root, count, route,
answer and verdict is the previous run's. With the class ladder held at the network's, the same
tree prints the previous 512 id. All four ids were reproduced by `misaka palw verify-context` in
release on the §9.5 host. The 128K run took 47.6 minutes beside a workspace build, peaking at
6.2 GB.

| | 512 | 4,096 | 32,768 | 131,072 |
|---|---|---|---|---|
| widest close (bytes) | 15,231 → 20,607 | 38,448 → 39,344 | 38,640 → 39,536 | 38,768 → 39,664 |
| `document_id` | `02818c10…` (was `d2364615…`) | `a54a5ee6…` (was `9c5a772c…`) | `0b8b191d…` (was `848d9352…`) | `d6464524…` (was `7bc3f88b…`) |
