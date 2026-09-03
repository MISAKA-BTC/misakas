# t11 Relaunch 5f — genesis card

**Branch** `palw-testnet-5f` · **tip at writing** `fc496331` · public at github.com/MISAKA-BTC/misakas

This is the card the cut is made from. Every number here is derived or measured; where a value is
someone's decision rather than an arithmetic consequence, it says so and says whose.

---

## 0. This is a RE-GENESIS, not an upgrade

The genesis UTXO set changes — the community premine entries are added — so the **genesis hash
moves**. There is no upgrade path from 5e: no earlier appdir and no earlier binary can join, and
the failure is at handshake rather than at a rule, which is strictly less legible than a ruleset
mismatch. Every host wipes.

Stating it as a rule and not as history matters, because the previous page said the genesis card
"has not moved since 5c, only the ruleset over it" — true as history, and read as a promise it was
never making.

---

## 1. Fences to arm at genesis

All eleven `Option<ForkActivation>` fences are `None` on every shipped preset today. Two are armed
at genesis and the rest stay `None`.

| fence | at genesis | why |
|---|---|---|
| `palw_context_ladder` | **ARM** | without it the registered class cannot price a wide row; the ladder is the whole point of the 512 registration |
| `palw_uncertified_weightless` | **ARM, `ForkActivation::always()`** | genesis is the ONLY moment this can be armed — `validate_palw_v2` refuses any other height |
| `palw_kary_court` | **ARM, `ForkActivation::always()`** | **ADR-0082's, does not exist yet.** Without it the registered row is admitted and UNPROSECUTABLE — see §3. A bare fence: no companion value, no bundle field. `dissection_arity` stays 2 on every preset and the fence overrides it with the derived arity. |
| `palw_prompt_ids_merkle` | `None` | ADR-0082's. Not needed at the registered width — flat ids are 82,080 against a budget of 83,333 — and arming it moves every free-prompt job id. **It becomes REQUIRED above about n_ctx 1,024**, so it is the fence to arm the day a wider row is registered, not before. |
| `palw_fp_decode_rules` | `None` | ADR-0082 stream H's (decode leaves earn, seeded argmax). Not a prosecutability condition and not on the acceptance path; deferred so the cut arms only what it must. |
| the other nine | `None` | nothing in this cut needs them; a fence armed without a shipping thing to obey it is the ADR-0065 D1 mistake |

**A fence that must be armed at genesis and does not exist yet is a scheduling fact, not a
contradiction** — it lands with ADR-0082 (§7), and the card names it now so the arming is not
discovered at cut time.

**Arming the ladder is TWO moves, not one.** `Params::palw_context_ladder` AND the bundle's
`PalwCourtParamsV2::max_step_leaf_count`. Setting one and not the other produces a build that
looks armed and prices the old row.

---

## 2. Class set at genesis

| class | registered | seated | note |
|---|---|---|---|
| BASE-0 floor | yes | yes | the floor must have a producer or DAA stops and the chain cannot leave the state by itself |
| dense A16 **512 row** | **yes** | yes | the only wide row registered — see the width argument below, which is not the one this card first gave |
| hybrid QWEN36 | **NO** | — | see below — this is now a correctness decision, not a margin one |
| demonstration class | yes | **SEAT AT GENESIS** | a class registered post-genesis has NO epoch budget until the next boundary |

**Why the hybrid row is not registered — and the reason is not the one either of us gave first.**
Two weaker arguments were on the table: a 0.43% admission margin, and graph-v4's composition being
`NotPriceable`. Both invite someone to re-open the question with a tighter geometry. The binding
reason does not:

    hybrid graph-v5, n_ctx   128:  200,604 bytes = 3 carriers
    hybrid graph-v5, n_ctx   256:  200,668 bytes = 3 carriers
    hybrid graph-v5, n_ctx   512:  200,732 bytes = 3 carriers

**The hybrid is three carriers at 128 tokens.** This is not a wide-row problem — it is the
recurrence's interval replay evidence, which does not shrink at any context. So the hybrid cannot
close on a shipped build at ANY width until W6/W7 open the split path (§5), and a registered class
whose disputes cannot be defended is worse than an absent one: the honest party loses by deadline.
Flat correctness fact, not a margin. *(Swept by 1c on a detached `palw-adr0082-f-cost`.)*

**No operator pubkey may appear in two `BondRegistered` rows.** The genesis tool PANICS on this,
it does not warn.

### Why 512, and why the first two answers were the wrong quantity

Three different numbers are all "tokens for a kind", and this card quoted the wrong one twice:

| quantity | value | what it actually answers |
|---|---|---|
| grammar floor | cad 38, music 60, scene 104 | the shortest legal non-degenerate answer — fits at n_ctx **128** |
| what the model gates needed | 256, both tiers | the width at which a checkpoint's own answers parsed |
| **what the shipped corpus costs** | cad 66, **music 261, scene 286** | **the answers a demonstration actually publishes** |

Only the third is binding. At n_ctx 256 the decode budget is 247, and **both the MIDI and the GLB
that were derived, validated through mido / pygltflib / numpy-stl and shown to a human are over
it** — 261 and 286 against 247. A launch sized on the floors or on the gates registers a class that
can express every kind and **cannot emit the artifacts the announcement is about**, and the failure
arrives as a worker refusal, after the announcement, on the one request the announcement invites.

At 512 the budget is 503 and the demonstration set closes. Whole corpus, for whoever revisits this:

    budget   119 (n_ctx  128)  ->   9/34 answers fit
    budget   247 (n_ctx  256)  ->  21/34
    budget   503 (n_ctx  512)  ->  24/34
    budget 1,015 (n_ctx 1024)  ->  25/34
    budget 2,039 (n_ctx 2048)  ->  28/34

The tail is grammar coverage rather than demonstration: `map/05-large-dungeon` is 11,222 tokens and
the `9x-*` entries exist to be refused.

**And the flatness makes the choice free** (§3): 80,440 bytes at 256 against 80,504 at 512 — one
carrier either way, both prosecutable, sixty-four bytes apart. There is no trade. 1,024 would cost
64 more and buy one more answer; 512 is taken because that is the width a demonstration has actually
been measured against, and registering a width nobody has measured is how this card got the previous
two answers wrong.

**And the width is right for BOTH lanes, measured rather than assumed.** The QWEN36 lane uses a
different tokenizer — the GGUF's own `tokenizer.ggml.*`, 248,320 tokens against the dense lane's
151,936 — so its costs did not have to be the dense ones. They are, exactly:

    cad/01-extrude-l-bracket     qwen36  66   dense  66   (+0)
    music/03-overlapping-melody  qwen36 261   dense 261   (+0)
    scene/02-hierarchy           qwen36 286   dense 286   (+0)

The reason it is durable rather than lucky: **a canonical DSL is ASCII JSON, and the two
vocabularies coincide over that range** — the larger one's extra 96,384 entries are elsewhere. A
grammar that ever admits non-ASCII string content does not inherit this and must re-measure.

*The agreement was only worth recording because the instrument was first shown capable of
disagreeing:* a tokenizer built from unread merges degrades to something byte-ish, and two degraded
tokenizers agree on everything — which would read exactly like a +0. The measurement asserts the
vocabularies differ in size AND that probes actually diverge (`Ω≈ç√∫` is 6 tokens against 5; two
musical-keyboard emoji, 6 against 2), so a degenerate load fails instead of producing a comfortable
+0.

**Third confirmation, from the artifact itself:** `qwen25-1.5b-a16.palwart` declares
`max_position = 512` in its own header. The registered row, the artifact on disk, and the
demonstration corpus all agree at 512. *(Corpus token counts measured by 1c with the shipped Qwen2.5
tokenizer; header decoded here.)*

---

## 3. ADR-0082 is a precondition, not an optimisation

This is the finding that changed the shape of the cut, and it is worth stating in full because the
earlier framing — "0082 makes it cheaper and wider, not possible" — was wrong and was believed by
three sessions.

Carrier is `PALW_OBJECT_CHUNK_MAX_BYTES` = 100,000 bytes. The `PalwStepBindingV2` the mempool
charges for but the court does not is 13,996 at its widest.

**Every graph-v5 figure below is UNDER THE DISSECTION COURT.** This is not a footnote: under the
shipped BINARY court a fused leaf's terminal check opens the whole history it reads — about 1 MB at
n_ctx 512, twelve to thirteen carriers — which §5 shuts. So a graph-v5 row registered with only the
ladder and weightless fences armed is admitted and **unprosecutable**, and the number that says
otherwise was measured under a court the build would not be running. `palw_kary_court` is what
makes the table true, which is why §1 arms it.

*(This correction is the same defect class as everything else on this card: a figure measured under
one configuration, quoted as if it held under another. It was caught by the session that derived
it, reading its own label — "dissection court, arity 16, Merkle prompt ids".)*

| row | close proof | + binding | carriers | fileable |
|---|---|---|---|---|
| graph-v2/v3 dense @ 512, binary court (today) | 1,154,673 | 1,168,669 | 14 | **no** |
| graph-v5 dense @ 512, **binary** court | ~1,000,000 | — | 12–13 | **no** |
| graph-v5 dense @ 512, **dissection** court | 80,504 | **94,500** | **1** | **yes** |
| graph-v5 hybrid @ 512, dissection court | 200,732 | — | 3 | no |

**How flat "flat" is, swept rather than interpolated** — graph-v5 dense, every registrable width:

| n_ctx | close proof | carriers | decode budget |
|---|---|---|---|
| 128 | 80,376 | 1 | 119 |
| 256 | 80,440 | 1 | 247 |
| 512 | 80,504 | 1 | 503 |
| 1,024 | 80,568 | 1 | 1,015 |
| 2,048 | 80,632 | 1 | 2,039 |
| 4,096 | 80,696 | 1 | 4,087 |

**64 bytes per doubling — thirty-two times the context for 320 bytes.** That is the sentence to put
in front of someone who has to believe it, and it is better than "the close stops growing with
n_ctx". It also means **512 is not a compromise**: 1,024 costs 64 more bytes and is equally
prosecutable. The width is chosen by what the model gate measured, not by what the close can
afford. *(Swept by 1c.)*

**Admissible and prosecutable are different properties**, and the tree had only ever measured the
first. Swept against what can actually be FILED:

    n_ctx 30 -> 81,849 bytes = 1 carrier   prosecutable
    n_ctx 32 -> 85,953 bytes = 2 carriers  admissible, disputes cannot be defended

The widest prosecutable dense row today is **n_ctx 30**, decode budget **21 tokens**, and the
grammar floors are cad 38, music 60, scene 104. **Nothing fits.** So without ADR-0082 the
demonstration is not expensive — it is impossible.

**Derived, and independently confirmed twice** (a 512-row table and the genesis-set derivation
landed on the same integers): dense A16 @ 512 = 1,154,673 binding `attn[10]`; hybrid QWEN36 @ 512 =
2,240,241 binding `attn[15]`; the set gives **27 = `DEFAULT_MAX_CLOSE_CHUNKS`**. The shipped
constant IS the derivation — nothing on this card corrects it.

`dissection_arity` stays **2** on every preset until its fence arms. The court's arity derivation
at the RC selects **4** by the move budget (48 moves, 2,160 of 3,000 DAA at the 45 clock), not the
ADR's worked 16.

---

## 4. The freeze and the single re-pin — ORDER MATTERS

`transformer_id` is a function of `source_tree_sha256`, which covers **every byte** under
`misaka-palw-derive/src/` — comments included. It moves silently and everything stays green when it
does. A published derivation whose id no longer reproduces is unverifiable, so this is the last
thing done before the cut and it is done ONCE.

**Order:**
1. Freeze `misaka-palw-derive/src/` completely. Three doc edits and one formatting pass are already
   inside the hashed bytes.
2. Run `cargo +<pinned> fmt --all` LAST among source-changing steps — formatting is inside the hash.
3. Then re-pin `transformer_id_pin` and `shipped_presets_have_pinned_fingerprints`, in one commit.
4. Nothing under `misaka-palw-derive/src/` may be touched after step 3, for any reason, including a
   typo in a comment.

**Two re-pins, two owners, no overlap.** The state version goes 18 → 19 on `palw-adr0082-impl` (the
court session gains its dissection phase) and the ADR-0043 goldens move with it — those are re-pinned
there, by the session that moves them. `transformer_id_pin` and
`shipped_presets_have_pinned_fingerprints` are re-pinned here, once, in step 3. Stated because a
fingerprint is not the sum of two diffs: two sessions re-pinning the same value against different
bases produce a third value, and this project has done exactly that before.

Anything quoting the old scene goldens is stale: `02-hierarchy` is **2736** (was 2716) and
`05-tetrahedral-rotations` is **17748** (was 17728).

---

## 5. Known-open, shipping anyway, stated so no page claims otherwise

**The split close is shut at the acceptance layer.** W5 built the state machine;
`palw_v2_validate_objects` refuses every `CourtCloseDeclared` unconditionally — *"no layer yet
verifies the declaring side's signature (ADR-0080 W6) — refused rather than trusted"*. There is
also nothing to sign (`PALW_COURT_V2_ALL_DOMAINS` carries no close context) and `close_digest` is
written and never read until W7. A refused lifecycle object is dropped **with the block standing**,
so filing spends the fee and opens nothing.

This is acceptable at genesis **only because** the registered class's close fits one carrier under
ADR-0082 and takes the single-carrier `CourtClosed` path, which is fully open and adjudicating. It
is the reason the hybrid row is not registered. `misaka-cli palw court-close` refuses the split
path before spending anything and names which limit stopped the operator.

**Faucet stays 0.5 tMSK.** The docs carry the real numbers instead: floor 11.2 MSK, A16 2,290,
QWEN36 3,868, plus an 8,333,316-sompi change floor. The faucet does not fund a bond and the pages
say so rather than implying it might.

---

## 6. Verification gates — every one must be green, and each must have been SEEN to fail

The rule earned today: **the gate you never ran locally is the gate you have never seen fail.**
`cargo fmt --all -- --check` was red on this branch for an unknown length of time, and every "CI is
green except the known pin" said this week was a statement about jobs that never ran.

| gate | command | note |
|---|---|---|
| format | `cargo +<pinned> fmt --all -- --check` | runs FIRST in CI; run it first locally |
| clippy | `cargo clippy --tests --benches --examples -- -D warnings` | must be the pinned toolchain, or it measures a different lint set |
| consensus core | `cargo test -p kaspa-consensus-core --lib` | 1645/1 — the one red is the fingerprint re-pin, which §4 closes |
| base0 | `cargo test -p misaka-palw-base0 --lib` | 264/0 |
| derive | `cargo test -p misaka-palw-derive` | **NOT `--lib`** — `--lib` builds no binaries, `palw-evm-runner` is absent, and ADR-0079's confinement gate refuses rather than falling back in-process. Those seven reds are the gate HOLDING. |
| cli | `cargo test -p misaka-cli` | 73/0 |
| artifact selftest | `scripts/misaka-palw-artifact-conformance.py selftest` | five damaged artifacts, each refused BY NAME — a test on the exit code alone would call four-of-five a pass |
| stranger | `scripts/misaka-palw-derive-stranger.py` | recomputes the bytes in Python, independently of the Rust |
| third party | `scripts/misaka-palw-artifact-thirdparty.py --require` | mido / pygltflib / numpy-stl; compares MEANING (enclosed volume, playback duration) against the DSL |
| model gate, dense | `palw-model-gate` | A16 lane only — declared in advance |
| model gate, QWEN36 | `palw-qwen36-model-gate` | needs the ChatML fix (§7) to pass through the production assembly |
| **prosecutability** | ADR-0082 stream I's end-to-end court drill | **This is the gate, and admission is not.** A graph-v5 leaf disputed to the bottom under the ARMED fence set, through `apply_object`: honest acquitted, forged convicted. F's admission arm refusing an unfenced `AttnFused` profile by name is a guard on the way in — useful, and not the property. The property is that a dispute can be carried to a verdict, and only the drill asserts it. |

**Why this row exists at all.** Every wrong turn on this card came from measuring admissibility and
reading it as usability: the 512 row, the hybrid's margin, the court the close was priced under.
Admissibility is what the tree kept measuring because it is what the tree made easy to measure. The
gate above is the one that would have caught all three.

---

## 7. Must land before the cut

- **ADR-0082 full implementation** — §3 makes this a precondition, not a preference. Includes the
  `palw_kary_court` fence itself (§1), and stream E's court wiring: the dissection phase on the
  court session, the `AttnDissection` close-proof arm, the deadline read, and a **state version bump
  18 → 19** because the session record gains a field.
- **NOBODY MAY APPEND A VARIANT TO `PalwConsensusObjectV2` ON 5f** until E's wiring lands. The Borsh
  discriminants are positional; W5 already appended two, and E appends three more. A fourth appended
  in parallel collides by number, and the collision is silent until two builds disagree about what
  an object is.
- **The QWEN36 lane's own chat renderer** — the lane serves Qwen3.5 through Qwen2.5's template. The
  model's own template appends a `<think>` preamble in BOTH modes; there is no branch that stops at
  `assistant\n`. Under the shipped assembly **4 of 8 correct answers were refused at column 1** and
  2 more burned their budget on an open reasoning trace. The preamble must be `Special(id)` — as
  text it becomes BPE pieces through ADR-0079 D7's `encode_without_specials`.
- **A way to certify the 512 row.** The A16 catalog is a fixed three-row table — `Qwen/Qwen2.5-1.5B`
  at n_ctx 16, `Qwen/Qwen2.5-Coder-1.5B-Instruct` at 18, `Qwen/Qwen2.5-1.5B/graph-v2` at 16 — and
  `palw-certify bind` takes only `--model-id`, with no `--n-ctx`, no `--class-id` and no
  `--artifact`. **So the certification tool cannot bind the class this card registers.**

  This is not a missing feature. It is **an identifier that does not identify**: a model id does not
  determine a width, `n_ctx` is inside the identity, and so everything downstream is a
  `ClassLaneCertified` for the wrong class or none at all. Under ADR-0075 the lane then ships
  CLOSED and the demonstration refuses on the first request — a refusal that reads to whoever runs
  it exactly like the width wall, which is the failure this card exists to prevent.

  **Fixed by naming the class, not by adding a row.** A 512 catalog row would make the drill green
  and is unfalsifiable: a wrong width binds to the wrong class silently, where a named width fails
  to bind. The table has already failed in that exact direction — its own comment marks
  **n_ctx 17 BURNED** by a 2026-08-28 mispairing that registered a class on chain against the
  genesis constant, past a green suite. So `bind` gains `--artifact`, deriving the profile by
  calling what the panel calls rather than writing a second spelling of it. The underlying defect is
  one class root spelled two ways — from the artifact's inventory and from a constant — with nothing
  forcing them equal, which is the A16 genesis root defect again.

  *It is in no test because no test registers a class it did not also define.* A rehearsal found it.

  *Two things this is NOT, both checked in the source rather than reasoned about:* it is not the
  court — `shape_profile_id` hashes the profile's borsh and no court field is in that struct, and
  `canonical_classes_v1`'s second line is `let _ = court;`, so the court parameter is decorative
  with respect to class identity. W3's `max_close_chunks` is not implicated.
- **Toolchain pinned** and the CI gates runnable in one local command
- **The single re-pin**, in the order of §4

---

## 8. Announcement wording — the claims the evidence actually carries

- **"The QWEN36 lane produced a grammar-valid description" is what the evidence carries.
  "Qwen3.6 wrote this file" is a different sentence.** The 35B weights are not on the machine that
  measured this (19 MB of metadata and an empty directory); what ran is `qwen35-2b.palwq36`, the
  QWEN36 lane carrying a 2B model.
- **512.** Not "thousands of tokens" for the registered row's width — 512, with ~500 decode tokens
  and about 1,200 bytes of answer.
- **"Thousands of tokens" is exact for the flat close; "tens of thousands" is not.** A graph-v5
  dense close is flat except the prompt-id term only to about n_ctx 4,096 — 80,504 at 512 and
  80,696 at 4,096, **192 bytes for eight times the context**. At 32,768 the binding node becomes
  the embedding gather and the generated-token ids are a second context-linear term ADR-0082 D5
  does not anchor; it Merkle-izes the prompt ids only.
- **The model's answer IS the artifact's source, canonicalized** — this is the central claim and it
  is now re-runnable: `palw-model-gate` and `palw-qwen36-model-gate` ship, and
  `docs/evidence-qwen36-model-gate/` carries the answer bytes and the artifacts derived from them.
- **5e had known unpatched defects while it ran.** The security-lane commit messages describe them
  and they are public. Say so; do not let someone else say it first.
