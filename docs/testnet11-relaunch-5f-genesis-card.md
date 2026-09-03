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

### The name is already contested, and the relaunch adds one more claimant

Measured on the live fleet, six hours of `misaka-t11-node0`'s own log:

    genesis mismatches      3,793
    consensus params mismatches   735
    distinct foreign genesis hashes seen   at least four
      4b619a1a…  1,440     8d2002cc…  1,396  (Relaunch 4's)
      c664a224…    612     08e9c8a4…    345  (this node's own, seen from the other side)
    the node's own peers    3 of 8 outgoing, inbound accepted

So the node is **not** isolated — it has working peers — and at the same time it is rejecting
thousands of dials from nodes that answer to `misaka-testnet-11` and build on something else. **At
least four chains share this network name right now**, and 5f will be the fifth.

Two consequences for the announcement, neither optional:

1. **A joiner must wipe.** Not "should" — an old appdir cannot handshake, and the failure it prints
   is a genesis or params mismatch, which reads like a bug in the new build rather than the expected
   result of not wiping.
2. **The old chains keep answering.** Somebody who points an old node at a seed will keep syncing
   something, and it will not be this network. The genesis hash is the only thing that tells them
   apart, so it belongs in the announcement as a value to check, not as trivia.

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

**Two ceilings, and the card quoted only one of them.** On the CLOSE side the flatness makes width
nearly free: 80,440 bytes at 256 against 80,504 at 512, one carrier either way, sixty-four bytes
apart. That is what this card said, and it said it as though 1,024 were simply the next rung.

**It is not, on the LADDER side.** At the chosen 2^26 the widest admissible row is **574** — so 512
fits with about twelve per cent of margin and **1,024 does not fit at all.** 1,024 needs 2^28. So
512 is not merely "the width a demonstration was measured against"; it is also close to the ceiling
of the ladder the genesis is arming, and the next rung is a different cap rather than a rounding.

Both sentences are true and only together are they the answer. A width has to clear the close budget
AND the ladder, and this card had been reasoning about the first while quoting the second's
numbers — the fifth time a figure measured under one configuration has been read as though it held
under another, and the first where the figure was this card's own.

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

### …and the same thing again, one axis over: the EVIDENCE ROUTE

The court must be able to play whichever route the disputed position needs, and the two routes do
not cost the same. Dense graph-v5 @ 512, dissection court, 2^32 ladder, kv_dim-wide tiles:

| route | bottom opening | whole close | chunks |
|---|---|---|---|
| checkpoint | 41,997 | 82,719 | **1** |
| cache-write (rows after the last checkpoint, one leaf per row with its own 2 KiB path) | 175,297 | 216,019 | **3** |

**And the dense tier has no checkpoints at prefill positions today** — the leg checkpoints per
decode call. So a dispute at any prefill position, or at a decode position whose tile straddles the
last checkpoint, is the three-chunk close, which §5 cannot file.

**Therefore, as things stand, the dense row is prosecutable only for the positions the checkpoint
route happens to cover, and that is not "prosecutable".** The honest sentence until this closes is:
*three carriers on the cache-write route, one on the checkpoint route.*

**Stream K has landed and the claim is now unconditional.** With a checkpoint at every position the
bottom opening is **40,461 bytes = 0.49 carriers at EVERY position class** — prefill, first decode,
tile-aligned, straddling a tile boundary, and last — measured per class rather than averaged. The
close falls from 216,019 (3 chunks) to **82,719 = ONE chunk**, binding `attn[7] AttnFused`, and
82,911 at n_ctx 4,096. Per-position retention is **zero**: the fold keeps no chunk bytes, where a
chunk-retaining capture at 16 positions would have held 1,114,112.

**And the cache-write route was not merely expensive — it was unsound.** K found a K/V series swap
admissible on it, which convicts an honest executor. It is REFUSED now for any class that
checkpoints every position, so the route that could not be carried is also the route that could not
be trusted, and removing it closes the arming blocker for `palw_kary_court` rather than working
around it.

One design fact to carry: the anchor sits at **p+1** — the checkpoint written once position p's own
K/V rows exist — not at p−1 plus a residue. That single change took the dense close from 93,367
(2 chunks) to 82,719 (1).

Hybrid graph-v5 is 274,460 = 4 chunks on the same derivation, bound by the recurrence replay
evidence. It is unregistered at genesis (§2) and will need W6/W7's split acceptance regardless.

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

**The re-pins, by owner. Nothing touches both sides.**

| pin | where | owner |
|---|---|---|
| `transformer_id_pin` | `misaka-palw-derive/tests/` | here, at the cut |
| `shipped_presets_have_pinned_fingerprints` | `consensus/core/src/config/params.rs` | here, at the cut |
| `golden_vector_ids_are_frozen` | `consensus/core/src/palw_freeprompt_v3.rs` — ADR-0082 stream H gave the job two fields | here, at the cut |
| `PALW_RC_COURT_E2E_ROOT_BYTES` | `consensus/core/src/palw_e2e_adjudicability.rs` | here, **FIRST** — see the ordering below |
| state version 18 → 19, ADR-0043 goldens | `palw-adr0082-impl` | 5b, on that branch |

**The e2e root is re-pinned BEFORE the fingerprint, because it is INSIDE it.** `consensus_params_id`
reads the pinned `PALW_RC_COURT_E2E_ROOT_BYTES`, so re-pinning the fingerprint first produces a
value computed from a stale root — correct-looking, self-consistent, and wrong. Order: e2e root,
then fingerprint, then everything else.

**And the trap that ordering hides, which cost a measurement today.** "Does adding a fourth
certified family move `consensus_params_id`?" was measured as **no** — byte-identical on both
networks, with the change and with it stashed. The measurement was correct and the conclusion was
false: the fingerprint was unchanged *because the build was not yet self-consistent*. Updating the
root pin, which the build requires anyway, moves it:

    testnet-11  a090885af5856071…  ->  404c624568360b22…
    devnet      7acb81ebd3c8d942…  ->  67b0e2ebcb02c7e5…

**Measuring whether a change moves a derived value, while a pin that value is derived FROM is
stale, measures a build nobody will ever run.** The constant's own doc said the root is inside every
RC network's params id; it was read after the measurement rather than before.

**The fingerprint moves under you, so re-pin LAST and never early.** Measured twice a few hours
apart on this branch: `a7baab79…` was the pin, one reading gave `d201a54f…`, and the next gave
`4e0fe90b…` (devnet `84153175…` → `b84ea8cf…` → `6dbad795…`). Nothing was wrong either time — the
value is a function of the consensus parameters and every merge moves it. A re-pin taken before the
last merge lands is a value that was true when it was read and false when it shipped.

**Careful with the third**: `golden_vector_ids_are_frozen` exists TWICE — `palw_freeprompt_v3.rs:1521`
and `palw_derived_v1.rs:441`. One name, two homes, in the same crate. Only the free-prompt one moved.
Re-pinning by name rather than by module is how the wrong one gets rewritten, and this is the same
one-thing-two-homes shape that produced the two ladder caps and the class root spelled two ways.

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

> **STALE AS OF THE ADR-0082 MERGE — do not cut from this paragraph.** What follows describes the
> release branch today, where W6/W7/W10 are absent. On the branch being merged they are LANDED and
> **the split-close path is OPEN**: `max_close_chunks` 27 on the RC and 1 on devnet, with an
> assembly deposit. The owning session is handing over replacement text as a patch note, and this
> section must carry what SHIPS before the cut. It is left here rather than deleted because a
> reader comparing the two branches needs to see which state each is in — and because a card that
> silently updated a paragraph it had been cut from twice would be the worst possible instance of
> this project's own defect.
>
> **A critical on that now-open path, from the audit:** the block's single court slot
> (`PALW_COURT_CLOSE_MAX_PER_BLOCK = 1`) is spent by an UNAUTHENTICATED object before validation and
> before the fence — one minimum-fee transaction per block carrying
> `CourtAttnRootClaimed { session_id: 0, signature: [1] }` first in order denies every
> `CourtCloseChunk` completion network-wide. **A close denied through its assembly window is not a
> delay; it is a conviction of the declarer** — the challenger loses its reserve and deposit, the
> executor is void-and-slashed. A dormant fence makes it worse rather than safer: refused later,
> still counted. It must be closed before the split path ships, not before it is armed.

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
| consensus core | `cargo test -p kaspa-consensus-core --lib` | **1646/2, and both reds are load-bearing** — see below |
| base0 | `cargo test -p misaka-palw-base0 --lib` | 264/0 |
| derive | `cargo test -p misaka-palw-derive` | **NOT `--lib`** — `--lib` builds no binaries, `palw-evm-runner` is absent, and ADR-0079's confinement gate refuses rather than falling back in-process. Those seven reds are the gate HOLDING. |
| cli | `cargo test -p misaka-cli` | 73/0 |
| artifact selftest | `scripts/misaka-palw-artifact-conformance.py selftest` | five damaged artifacts, each refused BY NAME — a test on the exit code alone would call four-of-five a pass |
| stranger | `scripts/misaka-palw-derive-stranger.py` | recomputes the bytes in Python, independently of the Rust |
| third party | `scripts/misaka-palw-artifact-thirdparty.py --require` | mido / pygltflib / numpy-stl; compares MEANING (enclosed volume, playback duration) against the DSL |
| model gate, dense | `palw-model-gate` | A16 lane only — declared in advance |
| model gate, QWEN36 | `palw-qwen36-model-gate` | needs the ChatML fix (§7) to pass through the production assembly |
| fused-attention guard | `verify_class_admission_v5` | Refuses an `AttnFused` profile unless `palw_kary_court_active_at` — `FusedAttentionNeedsTheKaryCourt`, *"the class carries a fused attention site and this ruleset's court has no dissection to try it with"*; and `PricedForADifferentCourt { priced, court }` when the registered cost shape's arity is not the court's. **A guard on the way in, beside the drill and not instead of it.** |
| **prosecutability** | ADR-0082 stream I's end-to-end court drill | **This is the gate, and admission is not.** A graph-v5 leaf disputed to the bottom under the ARMED fence set, through `apply_object`: honest acquitted, forged convicted. F's admission arm refusing an unfenced `AttnFused` profile by name is a guard on the way in — useful, and not the property. The property is that a dispute can be carried to a verdict, and only the drill asserts it. |

**The whole workspace, measured: 3,809 tests run, 3,807 passed, 2 failed** —
`cargo nextest run --no-fail-fast`, 720 seconds. Both failures are the pins below.

**Run it with `--no-fail-fast` or do not quote it.** The default run reported *"353/3809 tests run:
352 passed, 1 failed"* and stopped at the first pin. "One red" was true and said nothing about the
other 3,456 tests, which had not executed. A count of failures among tests that ran is not a count
of failures, and the gap between the two was three and a half thousand.

**The two expected reds, and the condition that closes each.** A branch with unexplained reds has
no gate; a branch with reds nobody wrote down has a worse one, because the next person greens them.

| red | closes when |
|---|---|
| `shipped_presets_have_pinned_fingerprints` | §4's single re-pin, after the freeze. Owned here. |
| `the_shipped_ruleset_admits_the_row_the_genesis_registers` | the ruleset's `COURT_MAX_STEP_LEAVES` moves to a cap that admits n_ctx 512 — **2^26 is the smallest**. Owned by whoever makes that change; it goes green by measurement, not by assertion. |

**The second red is the whole width story and it deserves its own sentence.** W1b made the executor
read the ruleset's ladder instead of a hardcoded constant — and the ruleset's field is *set to that
same constant*, `COURT_MAX_STEP_LEAVES = PALW_STEP_MAX_LEAVES`. So **W1b moved no width at all.** It
converted a constant nobody could choose into a value somebody has to choose, which is exactly the
half it was supposed to do. "W1b landed" and "the width moved" are two claims and only the first is
true today. Until the second is, the registered class is capped at **39 positions** — below `cad`'s
38-token floor once any prefill is counted, and nowhere near music 60 or scene 104.

Measured, and reproduced independently from two crates by two sessions:

| ruleset cap | widest admissible n_ctx (A16) | admits the registered 512 row |
|---|---|---|
| 2^22 (shipped today) | **39** | no — needs 59,000,848, has 4,194,304 |
| 2^23 | 79 | no |
| 2^24 | 156 | no — opens every grammar floor, at a class narrower than the registered one |
| **2^26** | **574** | **yes**, with 12% headroom |
| 2^28 | 1,833 | yes |

2^24 is the trap: it opens cad 38, music 60 and scene 104 and still tops out at 156, so it would
open MIDI and 3D **at a narrower class than the one being registered** — which means deriving a
third class id, which is the loop §2 exists to end.

`the_registered_row_names_the_ladder_it_needs` pins that table and is GREEN; it is arithmetic about
profiles and holds whatever any network froze. The red one is the other question.

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

  **Both ends now measured, not inferred:**

      registered by the panel   71bbb755…   the catalog's graph-v2 row at n_ctx 16
      certified by bind         8d2e6f16…   the artifact's own row at 512

  The registered value is printed by the panel that computed it rather than scraped from a log —
  an earlier reading came from `grep -oE '[0-9a-f]{128}' | tail -1` and was returning block hashes.
  **A value an operator is asked to match against a certificate has to be printed by the thing that
  computed it.**

  *It is in no test because no test registers a class it did not also define.* A rehearsal found it.

- **A drill fixture for the fused attention kernel, and it must land BEFORE the family's union.**
  `graph-v5@512` reaches ten kernels; nine are covered by the certified dense family and
  `09b81d17ed5a73ef…` is in none. So no `FamilyCertified` can be produced for the row and both lanes
  ship closed.

  §2's union ruling puts that kernel in the family's declared set. **A family that declares a kernel
  it has no fixture to drill is a certificate asserting an adjudication nobody has performed** —
  strictly worse than the honest refusal, because today the class cannot be certified and says so,
  and with the union alone it would be certified and the assertion would be empty. The first person
  to discover it would be a challenger unable to open a dispute over a kernel nobody had proven the
  court can score.

  And the fixture has to be judged by what it PROVES, not by existing: **a fault vector the court
  scores identically to a correct execution certifies nothing, and that failure looks exactly like
  success.** If it cannot be authored in this cut, the choice — register the row uncertified, or do
  not register it — is a genesis decision and belongs here, not in a declaration.

  *Two things this is NOT, both checked in the source rather than reasoned about:* it is not the
  court — `shape_profile_id` hashes the profile's borsh and no court field is in that struct, and
  `canonical_classes_v1`'s second line is `let _ = court;`, so the court parameter is decorative
  with respect to class identity. W3's `max_close_chunks` is not implicated.
- **Re-bind the dense artifact's tokenizer — from the INSTRUCT checkpoint, not the base one.** The
  shipped `qwen25-1.5b-a16.palwart` carries 64 zero bytes where the tokenizer commitment goes, and a
  zero commitment pins nothing: a replay with a different `tokenizer.json` produces the same
  artifact. Re-binding needs a re-conversion.

  **It must be `Qwen2.5-1.5B-Instruct`, sha256 `dd924a11…`, not the base checkpoint.** The file on
  this Mac is the BASE model — `a961db72…`, `config.json` 684 bytes; Instruct is `dd924a11…` with a
  660-byte config. **Both safetensors are exactly 3,087,467,144 bytes, so size does not distinguish
  them**, and the `tokenizer.json` is identical in both repos, so a tokenizer commitment computed
  from either reproduces the same value and confirms nothing about which weights were read.

  Converting the base checkpoint produces a *valid, faithful, deterministic* artifact — same size,
  same 434,440 operands, same class id `8d2e6f16…` through `bind --artifact` — whose weight body
  differs from the shipped one in **484,183,979 bytes as ±1 in int8**, which is exactly what a
  fine-tune delta looks like after quantization. Its inventory root is `2246a380…` against the
  shipped `1a7457f1…`.

  **Nothing derived from the base checkpoint may be registered.** Expect digest `c00faa48…` from the
  Instruct conversion, with only the commitment field moving.

  **Do it on `ibm`, where the inputs already are** — `/root/palw-class/qwen25-src/` holds the Instruct
  config (660 B), safetensors (3,087,467,144 B, sha256 `dd924a11…` confirmed on the machine that ran
  the original conversion) and tokenizer (7,031,645 B), and `/root/qwen25-pipeline.sh` is the script
  that produced the shipped file. Nobody downloads 3 GB.

  **Provenance, closed except one line.** The conversion ran **2026-08-27 17:10–17:20** on ibm from
  `Qwen2.5-1.5B-Instruct`, and the artifact was then distributed as a binary — identical sha256 on
  ibm, C and the operator box. The 08-30 23:21 timestamp on this Mac is the *copy landing*, three
  days later, which is why a local mtime said nothing about provenance. **What is still unrecorded is
  the converter REVISION**: the pipeline log records the build succeeding, not what it built from.
  That is answerable by walking `/root/misakas-t11r`'s reflog — unknown rather than unknowable — and
  the quantization code has not moved since.

  *Recorded because the trap is well disguised: two checkpoints of identical size, a shared
  tokenizer, and a converter that is deterministic and correct on both. The class id survives the
  swap — it hashes the PROFILE, and no weight is in the profile — so `bind --artifact` reports the
  same class for the wrong model. Only a weight-derived value, or the checkpoint's own sha256,
  tells them apart.*
- ~~Re-bind the dense artifact's tokenizer~~ — **DONE, and characterised to the byte.**

  | | value |
  |---|---|
  | input | `Qwen2.5-1.5B-Instruct`, `sha256 dd924a11…` verified on this machine after transfer |
  | tokenizer commitment | was 64 zero bytes at offset 1,777,209,032; now `fa9a4352…` |
  | container digest | `c00faa48…` → **`158314b5…`** (a genesis input) |
  | everything else | **byte-identical** — 128 bytes differ in a 1,795,427,276-byte file, and they are those two 64-byte fields |
  | faithfulness | 45/57 top-1, unchanged — the same weights |

  **And the reproducibility claim is now proven rather than argued.** Running the ORIGINAL converter
  on ibm against the same checkpoint reproduced the shipped artifact with **zero differing bytes**.
  So `qwen25-convert`'s own doc — *"a verifier re-runs this and compares the class id, which is why
  the conversion has to be bit-reproducible"* — is a property this build has, demonstrated on two
  machines with two binaries.

  The bound artifact is at `scratchpad/reconvert/instruct-bound.palwart` and must replace the
  shipped one at the cut; `from_registered_profile` refuses the unbound file, so the dense SDK path
  does not work until it does.
- **Toolchain pinned** and the CI gates runnable in one local command
- **The single re-pin**, in the order of §4

---

## 7b. Arming `palw_kary_court` — a checklist, not a sentence

§1 arms this fence at genesis. It must not be armed until every line here is closed, and each one
is a finding from the audit wave rather than a precaution.

- [ ] **Five criticals in the dissection court's bindings.** All one shape — *a derived value
      computed and not compared*: the root claim's `history_positions` taken from the wire; `S*`
      unvalidated, giving an i64 overflow panic **inside block validation**; bottom openings unbound
      to coordinates; the anchor unbound to the derived checkpoint; and no clock on the responder's
      root claim, so silence at Terminal wins challenger-side.
- [ ] **The engine executes the real fused node on the registered artifact, not only synthetic
      material.** The refusal — "per-layer declares 24 against 27 recorded" — is
      `profile.attn_nodes.len()` of a graph-v5 layer table (**27 − 3, the fusion's net removal**)
      against a hard-coded 27-row v4 program in the plan-LESS route. The PLANNED route already
      executes the fused site and is bit-equal to the reference on real geometry; only
      `Qwen25A16Backend::new` refuses, and chain-registered classes go through
      `from_registered_profile`, which plans. Until it lands, a `FamilyCertified` would cover a
      kernel nothing has executed on the row the genesis registers.
- [ ] **`(v − max) << up` wraps i64 before the clamp** in `softmax_shifted` and `a16_attn_exp_one`
      for `up_bits ≥ 47` — a key 40,000 below the maximum receives full weight. It moves committed
      values only for classes at that width, so **the fix is free exactly now and not after a v5 row
      is registered.** Confirm the shipped artifact's `up_bits` either way.
- [ ] **The window must fit a dispute a session actually PLAYS.** The derivation prices a k-ary LEAF
      ladder no session plays — the ladder is binary and only the history search is k-ary. At RC
      numbers the played dispute is **60 moves × 51 + 216 reserve = 3,276 against a 3,000 window**,
      so Z4 is violated and **an honest prosecution times out.** Being re-derived; **if no arity fits
      the 512 row inside the RC window, that is a genesis decision and comes back here.**
- [ ] **The arity has two spellings** — a bundle literal of 2 against a derived 4, with admission
      comparing the wrong one and nothing asserting they agree. Fourth instance of one-thing-two-homes.
- [ ] **The cost walk caps at 2^22** (`genesis_anchored_v1`), so the 512 row is refused with
      `TooManyLeaves` on a 2^26 network — the same 2^22-against-2^26 mismatch as the executor's
      ladder, in the cost arm.
- [ ] **Post-genesis registration calls `verify_class_admission_v3`** — no court, no ladder — so no
      graph-v5 class can ever register post-genesis; and **genesis minting runs no admission gate at
      all.**
- [ ] **A second pinned cross-machine determinism digest for the v5 base** (ADR-0067 D5 is pinned
      only for the v2 fuzz corpus). A new pin produced by a run, not a moved one.

*The 27 − 3 above also settles §2's family ruling by measurement: the fusion removes four kernels
and adds one, so a family derived from the v5 row alone would drop `ATTN_SCORES` and `ATTN_VALUES`
and stop covering the rows already on chain. The family's `kernel_ids` are the UNION over every row
the lineage ships.*

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
- **The dense tier's faithfulness is 45 of 57, not 57 of 57.** The shipped artifact's own conversion
  log records `a16 top-1 agree 45/57`, `top-5 contains 56/57`, `rank corr (100) 0.8932` against the
  float reference, and calls that FAITHFUL — which is the bar the class passed, measured at
  conversion time rather than asserted. Say the number if the subject comes up; do not let a page
  imply the quantized class reproduces the reference exactly. (An independent conversion here of the
  *base* checkpoint landed at 44/57 and 0.8627, so the figure is characteristic of the quantization
  rather than of one lucky run.)
- **The model's answer IS the artifact's source, canonicalized** — this is the central claim and it
  is now re-runnable: `palw-model-gate` and `palw-qwen36-model-gate` ship, and
  `docs/evidence-qwen36-model-gate/` carries the answer bytes and the artifacts derived from them.
- **5e had known unpatched defects while it ran.** The security-lane commit messages describe them
  and they are public. Say so; do not let someone else say it first.
