# ADR-0078: What was made from it is committed; the thing itself never rides

**Status:** PROPOSED (2026-09-02). Written to draw a line ADR-0077 needed drawn: implementing
ADR-0077 makes an inference at a usable width a certified, spendable claim. It does NOT make
"generate a 3D model with Qwen3.6, verified end to end" true. The mesh is not the inference. This
ADR is the layer above the receipt — the model's output turned into a thing a person keeps — and
its rule for the chain is the title. Nothing here bears weight, moves a preset, or changes how a
claim is certified; the consensus surface is one compact object per derivation, and it is
introduced in a ruleset move of its own.
**Builds on:** ADR-0077 (R0: one inference is one answer and one claim; R1: the artifact to the
user, the receipt to the chain), ADR-0044 (the receipt lane; `output_root` is the commitment to
the output ids, never the tokens), ADR-0074 Decision 1 (the `User` prompt mode), ADR-0075
(lifecycle objects; `ObjectChunk` for anything above one carrier), ADR-0053 (one execution family:
pure Rust in the tree, integer arithmetic, because float is unadjudicable), ADR-0026 (borrow the
architecture, refuse the tolerant proof model).
**Leaves open, by name:** weight for a transformer's own computation (§3 Decision 7); it is the
door this ADR builds and does not walk through.

## 1. The line, and why it has to be drawn

ADR-0077's scope is exactly this chain of objects:

```text
Qwen3.6 (or A16, or any certified class)
  → an inference at the class's width (a prompt a person typed, an answer they keep)
  → checkpoint-anchored capture, one interval opened to each seat
  → claim → certified → receipt block
```

A person who asks for "ten NPCs for this game" gets JSON back, and that JSON's generation is what
the chain certified. Everything after it — the renderer that makes meshes of the JSON, the GLB the
game loads, a PNG, a STEP file, a compiled program, a MIDI file, a simulation's trace — is a
computation the model did not perform and the court cannot try. Two wrong answers are available
here and this ADR refuses both:

* **Carry the thing.** A GLB is megabytes; a block carries ~125 KB of transaction bytes and a
  lifecycle object at most eight 100,000-byte chunks (ADR-0075 Decision 14). The chain is not a
  file store, and a design that made it one would price every artifact against the mempool.
* **Pretend the inference certifies the thing.** A receipt says the model produced these output
  ids under this class. It says nothing about what a renderer did with them. Reading it as a
  certificate of the mesh is the same category error as reading a block hash as a certificate of
  the transaction's meaning.

The right answer is the one ADR-0077's R1 already states for the receipt and this ADR extends one
layer up: **commit what the model made, and what was made from it — keep neither.** The chain
holds a derivation: which claim, which transformer, which bytes in, which bytes out, by hash. The
bytes stay with the person who asked for them.

## 2. The shape

```text
     the model's output ids          (committed by the claim: output_root)
              │
              ▼  render + canonicalize  (a registered grammar: grammar_id)
     canonical DSL bytes               (dsl_hash)
              │
              ▼  a deterministic transformer  (content-named: transformer_id)
     the artifact                      (artifact_hash, artifact_bytes)
              │
      ┌───────┴────────┐
      ▼                ▼
 to the user       DerivedArtifactV1 { claim_id, grammar_id, transformer_id,
 (GLB, PNG, STEP,                     dsl_hash, artifact_hash, artifact_bytes, kind }
  code, map, MIDI,                    + the executor's signature → the chain
  a trace)
```

One inference, one claim (ADR-0077 R0). Zero or more derivations of that claim, each a compact
object. The DSL is not a second language the model must learn: it is the model's answer, in a form
a grammar can canonicalize — JSON for a scene, source text for a program, a note list for music.
What makes the layer sound is not the DSL; it is that the transformer is a **pure function** the
way a PALW class is: integer or exact arithmetic, no clock, no randomness, no network, one build
named by its content, byte-identical on two architectures before anything may name it.

## 3. Decisions

**Decision 1 — a derived artifact is a derivation, committed; the artifact never rides.** The
chain accepts `DerivedArtifactV1` and stores it beside the claim it names. It never accepts the DSL
bytes or the artifact bytes as consensus carriage, in any chunking, under any size. A future ADR
that wants the chain to hold a thing has to argue against this sentence.

**Decision 2 — the DSL is the claim's output, canonicalized by a registered grammar.** The
transformer's input is the rendering of the ids the claim committed (`output_root` is
`output_commitment_v2` over exactly those ids, so a consumer holding the answer recomputes it),
passed through a grammar's canonicalizer — a pure function (whitespace, key order, number form,
nothing semantic) named by `grammar_id`. `dsl_hash = H(grammar_id ‖ canonical bytes)`. A parse
failure yields no derived object and nothing else: the inference still certifies and still mines,
because ADR-0077 R1 credits the computation, not what it happened to be good for. Nothing about
the prompt or the answer is changed to make it parse — F1 (ADR-0044) reaches this layer intact.

**Decision 3 — transformers are content-named pure functions, held to the family's discipline.**
A transformer is described by a manifest — `kind`, `grammar_id`, the build's source-tree hash, the
arithmetic discipline it declares (integer or exact rational; no `f32`/`f64` on any path that
reaches the output), and its output format's canonical writer — and `transformer_id = H(manifest)`.
It runs in the tree, pure Rust, no external runtime (the ADR-0053 rule, for the reason ADR-0053
gives: a float path is a path two honest hosts disagree on). Before a kind's transformer may be
named by any object, the drill runs it on the same DSL on two architectures and requires
byte-identical artifacts (the cross-device discipline the free-prompt lane already applies to
claims). A transformer that cannot pass that is not a transformer under this ADR; it is an
application, and applications are welcome to exist without a commitment.

**Decision 4 — the object, and what the chain checks.**

```rust
pub struct DerivedArtifactV1 {
    pub version: u16,
    pub network_domain: Hash64,
    pub claim_id: Hash64,          // the free-prompt claim whose output this derives from
    pub output_root: Hash64,       // MUST equal the claim's committed output_root (cross-check, not a second source)
    pub grammar_id: Hash64,
    pub transformer_id: Hash64,
    pub kind: u16,                 // the kind table, Decision 8
    pub dsl_hash: Hash64,
    pub artifact_hash: Hash64,
    pub artifact_bytes: u64,
    pub executor_pubkey: Vec<u8>,  // MUST equal the claim's executor bond key
}
// + signature; derived_id_v1 = H(canonical(object)) — total binding, every field in the preimage.
```

It rides a lifecycle transaction like ADR-0075's objects. At acceptance the transition checks:
the claim exists on this chain (in any phase from committed onward — a derivation of a claim
that later voids is a derivation of a voided claim, and says so when read); `output_root` equals
the claim's; the signer is the claim's executor; `(claim_id, transformer_id)` is unique;
`derived_per_claim ≤ PALW_DERIVED_MAX_PER_CLAIM` (4). It records the object beside the claim, in
the state root, and retires it with the claim (`CLAIM_RETIREMENT`). It credits **no weight, no
payment, no exposure**: the object is a statement, priced by its transaction fee, and a statement
does not need collateral to be useful — it needs to be checkable, which Decision 5 makes it.

**Decision 5 — verification belongs to the consumer, and the chain makes it possible.** Whoever
holds the answer (the user, or anyone the user hands it to) can check the whole derivation
without trusting the executor: recompute `output_root` from the ids and match the claim;
canonicalize under `grammar_id` and match `dsl_hash`; run the transformer named by
`transformer_id` and match `artifact_hash` and `artifact_bytes`. Every step is a pure function of
bytes the consumer has and ids the chain has. A false object is therefore publicly demonstrable
by anyone holding the DSL. Stated rather than hidden: in this ADR a demonstrated falsehood costs
the executor nothing on chain — no bond hangs on a derivation, because the chain cannot run an
arbitrary transformer and this ADR refuses to pretend it can. What a false derivation costs is
the one thing the object exists to carry: the executor's name on a provenance that anyone can
show is wrong. Decision 7 is the route to making it cost collateral.

**Decision 6 — delivery, and what is under a data-availability obligation.** The gateway
(ADR-0077 Decision 3/4) returns, in one response: the answer (the DSL, as text), the artifact (as
bytes, inline for small artifacts and by a fetch handle above a size the gateway states), and the
signed `DerivedArtifactV1` it submitted or would submit. The artifact is never under a DA
obligation — the chain has no use for it. The DSL may be, at the user's election: an executor
that opts a claim's DSL into the obligation serves it on request like a capture opening
(ADR-0077 Decision 8), so that third parties can verify a derivation they did not commission;
the default is off, because the DSL is the answer to the user's prompt and ADR-0044 Decision 8's
sentence about silently publishing prompts applies to answers word for word.

**Decision 7 — the door to weight: a transformer that is a step space is a class.** A
transformer whose computation is an integer step space — every operation a catalogued kernel,
every intermediate committable, checkpoint-anchored like a model's layers — can register as an
execution family through ADR-0075's route, drill, bind to the free-prompt lane, and then its
leaves are counted work: a simulation run, a rasterization, a mesh build would earn weight
exactly as an inference does, priced by the leaves it executed, tried by the same court. This
ADR builds the object and the naming that such a family would need and decides nothing about
which transformer goes first; the simulation kind is the obvious candidate because an integer
step simulator IS a step space with no translation. Until a kind takes that route, derivations
of it weigh nothing, which is the honest weight of a computation no court can try.

**Decision 8 — the kind table, v1.** One row per kind a person asked for; each names its DSL, its
transformer's discipline, its artifact, and what "verify" means. Non-goals are in the table on
purpose, because a kind that is not there is not covered, and a reader should not have to infer
it.

| kind | the DSL (the model's answer) | transformer | artifact | determinism basis | not covered |
|---|---|---|---|---|---|
| `scene` | Canonical Scene DSL: objects, fixed-point transforms, materials by name, hierarchy | fixed-point mesh builder + canonical glTF writer (buffer order, accessor order and padding fixed) | `.glb` | integer geometry; no float in the path | texture synthesis; physics |
| `image` | vector/procedural DSL: paths, fills, layers, integer coordinates | integer rasterizer + canonical PNG writer (fixed filter, fixed zlib level, chunk order) | `.png` | fixed-point coverage; no AA randomness | diffusion / pixel models — those are model classes (ADR-0075's route), not transformers |
| `cad` | parametric solid DSL: sketch → extrude → revolve → boolean | exact-rational or fixed-point kernel; canonical STL / STEP writer | `.stl`, `.step` | exact arithmetic; booleans are the hard part and a kernel that cannot make them exact does not ship the boolean | NURBS surfaces at v1 |
| `code` | source files + a build manifest (targets, tests) | a pinned, hermetic toolchain: no network, fixed clock, fixed locale and env, build tree hash in the manifest | build outputs + a test log | hermetic reproducible build; the toolchain's own reproducibility is the gate — a toolchain whose output varies across runs is excluded by name | anything that fetches at build time |
| `map` | tile / graph DSL: cells, edges, integer attributes | integer map compiler | the map file | pure function | — |
| `music` | note-list DSL: pitch, onset and duration in ticks, channels, programs | Standard MIDI File writer | `.mid` | SMF bytes are canonical | waveform synthesis (float DSP) |
| `simulation` | scenario DSL: entities, integer rules, a seed, a step count | integer step simulator | a trace + a summary | an integer state machine — the first candidate for Decision 7 | anything with floating-point dynamics |

A kind is versioned by its grammar and its transformer, never by editing a row: a new grammar or a
new build is a new `grammar_id` / `transformer_id`, and the old derivations stay checkable against
the old ids forever, which is the same reason a PALW class is its graph.

## 4. What this costs, stated before it is measured

* **Chain bytes.** One `DerivedArtifactV1` is a few hundred bytes; at most four per claim. No
  chunking is ever needed because nothing large is carried (Decision 1).
* **State.** One bounded table beside the claim table, retired on the claim's schedule; the state
  root grows by a bounded row per derivation.
* **Executor time.** The transformer's own run, after the inference. A scene build or a MIDI write
  is milliseconds; a hermetic build of a program is whatever the toolchain takes; a simulation is
  what it is, and the gateway streams the answer before the transformer starts, so a slow
  transformer never delays the text.
* **Consumer time.** Verification is one transformer run plus hashing, on bytes the consumer
  already holds.
* **Identity.** Introducing the object is a ruleset move (a testnet-11 relaunch); mainnet, PALW
  off, is untouched.

## 5. Invariants the tests must hold

```
X1   No consensus path accepts DSL bytes or artifact bytes as carriage, chunked or whole.
X2   A DerivedArtifactV1 is accepted only for a claim that exists on this chain, with the claim's
     own output_root and the claim's own executor key; (claim, transformer) is unique; at most
     PALW_DERIVED_MAX_PER_CLAIM per claim.
X3   A transformer named by any object is byte-identical on two architectures over the drill's
     DSL corpus, and declares no floating-point arithmetic on any path that reaches the output.
X4   A parse failure under a grammar produces no object and changes nothing about the claim.
X5   A derivation credits no weight, no payment and no exposure (Decision 7 is the only way it
     ever will, and it is not this ADR).
X6   From (answer ids, grammar_id, transformer_id) alone, a consumer recomputes output_root,
     dsl_hash and artifact_hash and reaches the object's values or a demonstrable mismatch.
X7   ADR-0077's R0 and R1 hold unchanged: one inference, one claim; the receipt's bytes and the
     seat's bytes are what they were.
```

## 6. Order of work

| unit | content | done when |
|---|---|---|
| Q-01 | `DerivedArtifactV1`, `derived_id_v1`, the kind table's ids | golden vectors; per-field mutation moves the id |
| Q-02 | the transition arm and the bounded table | X2 green; retirement with the claim |
| Q-03 | grammar + transformer for `scene` and `music` (the two with trivially canonical outputs) | X3 green on x86_64 and arm64 |
| Q-04 | the gateway's derivation step and one-response delivery | a browser request returns text, a GLB, and a signed object; X6 checked by the client |
| Q-05 | `map`, `image`, `simulation` transformers | X3 green each |
| Q-06 | `code` (hermetic toolchain) and `cad` (exact kernel, booleans last) | X3 green each; the kernel's boolean either exact or absent |
| Q-07 | the DA election for the DSL | served on request; off by default |
| Q-08 | Decision 7's first family — a simulation kind as a step space | its own ADR |

**Done when** a person asks a certified class for a scene, receives the JSON and the GLB, keeps
both, and anyone they hand the JSON to can recompute the claim's `output_root`, the `dsl_hash`
and the `artifact_hash` from the object on the chain — with the chain holding a few hundred bytes
and the GLB holding the megabytes.

## 7. Supersession

| what | disposition |
|---|---|
| ADR-0077 §2 "Where this ADR ends" | this ADR is the layer it points to |
| ADR-0077 R1 — the artifact goes to the user | honoured one layer up: the derived artifact goes to the user, its derivation to the chain |
| ADR-0075 Decision 14 — chunked carriage for objects above one carrier | not used: nothing here is above one carrier, by construction (Decision 1) |
| ADR-0053 — one execution family, pure Rust, integer | the transformer discipline (Decision 3) is the same rule applied to a non-model computation |
| ADR-0044 Decision 8 — prompts must not be silently published | applied to answers (Decision 6): DSL availability is the user's election, default off |

## 8. What is deliberately not decided

* **Which transformer becomes a family first** (Decision 7), and the step-space encoding of a
  rasterizer or a mesh builder — each is its own ADR with its own drill.
* **Slashing for a false derivation.** Without Decision 7 the chain cannot run the transformer,
  so it cannot convict; a bonded "I will pay if shown wrong" escrow with an off-chain
  demonstration is possible and deferred, because a court that cannot compute the verdict is a
  vote, and votes are what this lineage refuses.
* **Model classes for images and audio** — a diffusion model is a class, and its route is
  ADR-0075's, not this layer's.
* **Waveform synthesis, physics, texture generation** — floating-point by nature today; a
  fixed-point synthesizer or physics step would enter through Decision 3's gate like any other
  transformer.
* **Cross-claim derivations** (an artifact made from several answers) — an object with several
  `claim_id`s; not needed for any kind in the table, and left out until one needs it.

## 9. Number hygiene

This is ADR-0078; ADR-0077 is the last on this branch. A concurrent claimant renumbers the later
writer, per ADR-0036 Decision 5.
