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
| dense A16 **512 row** | **yes** | yes | the only wide row registered; both model gates needed 256, and at 512 the decode budget is 503 against grammar floors of 38 / 60 / 104 — all three fit with room |
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
