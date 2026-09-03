# t11 Relaunch 5f — genesis card

**Branch** `palw-testnet-5f` · **tip at writing** `fc496331` · public at github.com/MISAKA-BTC/misakas

This is the card the cut is made from. Every number here is derived or measured; where a value is
someone's decision rather than an arithmetic consequence, it says so and says whose.

> **Every `file.rs:NNNN` citation below resolves against `palw-adr0082-impl` at `aa049f96`, not
> against this branch.** The card describes the POST-MERGE cut, so it necessarily cites code 5f does
> not have yet — `palw_attn_court_v1.rs` does not exist here at all, and `prompt_ids_merkle` appears
> zero times in this branch's `params.rs`. That is legitimate and it is also exactly how six wrong
> citations got written in one sitting: read from one tree, committed to another, and wrong in a way
> no test suite can see. **`scripts/check-doc-citations.sh <doc> <tree>` resolves every citation and
> prints the line it lands on**; run it against the merged tree at the freeze and read the output,
> because a citation that resolves is not the same as a citation that is right. Caught by the
> session verifying this card, not by the one writing it.
>
> **And relabelling is not a null operation.** Declaring the card impl-relative fixed those six and
> silently INVERTED one that had been right against 5f — `golden_vector_ids_are_frozen` in
> `palw_freeprompt_v3.rs`, 1521 on this branch and **1643** on impl — inside the very sentence
> warning that the function has two homes. So the freeze check is the TWO-tree form:
> `check-doc-citations.sh <doc> <tree-a> <tree-b>` lists only the citations that resolve
> DIFFERENTLY, and those are the only ones any tree label can hurt. Seven of this card's citations
> are tree-dependent; everything it does not list is immune to the label.

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

    over 24.3 h on the public entry node (.t11, port 26311)
    genesis mismatches           18,034   from 10 distinct peers
    consensus params mismatches   3,074
    distinct FOREIGN genesis hashes    THREE — the fourth value in the log is ours
      4b619a1a…  8,352    8d2002cc…  6,377  (Relaunch 4's, still dialling)
      c664a224…  3,311    08e9c8a4…  18,040 as `local`, once per rejection line
    successful inbound connections    67, from 9 distinct peers; 6 connected now

The three foreign counts sum to 18,040 exactly, so they account for every line and there is no
fourth. **Ten stuck peers retrying every ~48 seconds produce eighteen thousand rejections** — which
is the figure to give a joiner, because "18,034 mismatches" reads like a swarm and ten misconfigured
nodes on a retry loop is what it is.

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

There are **fourteen** `Option<ForkActivation>` fences, not eleven, and "None on every shipped
preset" is true only of **testnet-11**. Measured:

    testnet-11   0 armed
    devnet       5 armed — palw_heartbeat, palw_attempt_work, palw_context_ladder,
                           palw_uncertified_weightless, palw_kary_court

**That is not merely a correction to a count — devnet already runs the exact configuration §1 tells
the operator to arm.** `palw_context_ladder`, `palw_uncertified_weightless` and `palw_kary_court`
are all `Some(ForkActivation::always())` there today. So the arming below is not an untried
combination being attempted for the first time on a public network: it is the devnet preset's
standing configuration, and `DEVNET_PARAMS` is the worked example to diff against when §7b's
checklist disagrees with what a build does.

Three are armed at genesis on testnet-11 and the rest stay `None`.

| fence | at genesis | why |
|---|---|---|
| `palw_context_ladder` | **`None` — DO NOT ARM** | **This row said ARM and its reason was false.** There is no `palw_context_ladder_active_at`: nothing reads the fence. Every use of that name is the *module*, the field, the `never()→None` normalisation, or `params.rs:3243` writing it into the FINGERPRINT. So arming it moves the fingerprint and gates nothing — and FG proves the reason wrong, because the 512 row is registered and priced at 2^26 **with this fence dormant**. The wide row prices from the bundle's `max_step_leaf_count`, which is the second of the "two moves" below; the first move is decoration. |
| `palw_uncertified_weightless` | **ARM, `ForkActivation::always()`** | genesis is the ONLY moment — `validate_palw_v2` refuses any other height (`params.rs:1490`). **And unlike the ladder, this one HAS a reader**: `palw_class_bears_weight_v2` (`palw_state_v2.rs:1183`) is `!uncertified_weightless \|\| share > 0`, called from `palw_claim_safe_contribution_v2` and the safe-weight accumulation at `:4589`, with the consistency check branching on it at `:4770`. It gates whether an uncertified class's `pwu` enters safe weight — ADR-0069 D7, on the fold that is the production path. |
| `palw_kary_court` | **ARM, `ForkActivation::always()`** | **ADR-0082's, does not exist yet.** Without it the registered row is admitted and UNPROSECUTABLE — see §3. A bare fence: no companion value, no bundle field. `dissection_arity` stays 2 on every preset and the fence overrides it with the derived arity. |
| `palw_prompt_ids_merkle` | `None` — **and it cannot be armed** | ADR-0082's. Not needed at the registered width: flat ids are 82,080 against a budget of 83,333. It becomes REQUIRED above about n_ctx 1,024 — but as of FD it is no longer a fence anyone may arm at that point: **`validate_palw_v2` REFUSES a ruleset that arms it** (`config/params.rs:1595`, on impl; 2310 is only the doc comment about it), because the commitment form it selects does not ship. Registering a wider row is therefore blocked on implementing the form, not on flipping the fence. |
| `palw_fp_decode_rules` | `None` | ADR-0082 stream H's (decode leaves earn, seeded argmax). Not a prosecutability condition and not on the acceptance path; deferred so the cut arms only what it must. |
| the other nine | `None` | nothing in this cut needs them; a fence armed without a shipping thing to obey it is the ADR-0065 D1 mistake |

**A fence that must be armed at genesis and does not exist yet is a scheduling fact, not a
contradiction** — it lands with ADR-0082 (§7), and the card names it now so the arming is not
discovered at cut time.

**The two fences look alike and are opposites — the difference is one grep, and it is the grep to
run before arming anything.** Both are `Option<ForkActivation>`, both were listed ARM here, both
reach `palw_ruleset_id_v2`. One is wired to the state fold that decides safe weight; the other is
wired to nothing. *The name of a fence tells you what it was meant to do; only its readers tell you
what it does.* The check is: does an accessor exist, and does anything outside `params.rs` call it?

**"Arming the ladder is TWO moves" was half right: only the SECOND move exists.** The bundle's
`PalwCourtParamsV2::max_step_leaf_count` is the whole mechanism; `Params::palw_context_ladder` has
no reader. Setting the fence and not the field produces a build that looks armed and prices the old
row — which was the real warning, and it survives. Setting the field is what moves the width.

*Arming a fence nothing reads is the ADR-0065 D1 mistake — a rule armed with no shipping thing to
obey it — which is the last row of this very table. I wrote the row and the warning against it in
the same section, and it took FG registering the row with the fence dormant to show which was
right.*

**And it is armed to `PALW_RC_COURT_MAX_STEP_LEAF_COUNT` = 2^26 — not to 2^32.** Naming the field
without naming its value is the same defect one level down. 2^32 is not available at this cut and
the refusal is arithmetic, not policy: at 2^32 the RC admits **no arity at all**, because the
cheapest honest exchange is 73 moves (at arity 32 or 64) and `73 × 42 = 3,066` is already past the
3,000-DAA window before the 216-DAA assembly reserve is added. `palw_court_arity_v1` returns `None`
there, which is ADR-0082 Z4's refusal working. Going deeper than 2^26 needs the window to grow, the
clock to shrink, or the leaf ladder to become k-ary for real — all three are genesis decisions, and
none of them is in this cut.

---

## 2. Class set at genesis

| class | registered | seated | note |
|---|---|---|---|
| BASE-0 floor | yes | yes | the floor must have a producer or DAA stops and the chain cannot leave the state by itself |
| dense A16 **512 row** | **yes** | yes | the only wide row registered — see the width argument below, which is not the one this card first gave |
| hybrid QWEN36 | **YES** | — | registered at 489‰ declared (957‰ after dense dilution). **This card excluded it for a reason that expired twice — see below.** |
| demonstration class | yes | **SEAT AT GENESIS** | a class registered post-genesis has NO epoch budget until the next boundary |

**Why the hybrid row IS registered, after this card spent two revisions saying it should not be.**
Two weak arguments were on the table first — a 0.43% admission margin, and graph-v4's composition
being `NotPriceable` — and were replaced by one that felt binding and was not:

    hybrid graph-v5, n_ctx   128:  200,604 bytes = 3 carriers
    hybrid graph-v5, n_ctx   256:  200,668 bytes = 3 carriers
    hybrid graph-v5, n_ctx   512:  200,732 bytes = 3 carriers

**Three carriers, at every width — against a ceiling of TWENTY-SEVEN.** That is 24 to spare, and
the argument dies there. It was written when a class needing more than one carrier could not file at
all; W6's two acceptance checks are in `palw_court_v2` and the split path is open. **I carried the
conclusion past the change that falsified it, twice, restating it each time as though restating were
checking.** Removing the row now would move the dilution and collateral arithmetic to satisfy a
paragraph, which is the worst reason there is to move a genesis input.

*The superseded argument is kept below, because the shape of the mistake is the useful part: a
disqualification that was true, became false, and went on reading as true because nothing in the
sentence expired.*

Superseded — this is not a wide-row problem — it is the
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
| grammar floor | cad 38, music 60, scene 104 | the shortest legal non-degenerate answer. **NOT "fits at n_ctx 128"** — see the prompt row |
| **the prompt itself** | **134 tokens**, ChatML, measured on the live gateway | added to every floor: cad **172**, music **194**, scene **238**. Nothing fits at 128 |
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

**And a third confirmation arrived from the chain itself, not from a corpus.** Stage 4 of the
end-to-end drill PASSED — the panel registered the class, every validator carried `FamilyCertified`
and `ClassLaneCertified`, the chain graded both, and `/health` reported `fp_certified` true and the
lane open. Stage 5 then refused a **one-token** job:

    prompt 134 + decode ceiling 1 exceeds max_context_tokens 16

That is the whole certification path working and the width being the only thing left — and it is
what turns the 134 into a measurement rather than an estimate. It also says the first row of the
table above was quoted wrong wherever it appeared: a grammar floor is what the ANSWER costs, and no
job is only its answer. `scene`'s 104 needs 238 positions to be asked for at all.

**So "why 512" has a measured answer in three independent directions**: 238 is the floor once the
prompt is counted, 286 is what the shipped `scene` corpus actually costs, and 574 is the widest row
the 2^26 ladder admits. 512 is the first artifact-stated width above the first two and inside the
third. It is not a round number anybody liked. (2^24's 156 would not have covered even `scene`'s
238, which confirms the ladder choice from the same direction.)

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

> **Every close figure on this card carries its ARITY *and its PROMPT-IDS FORM*, because three of
> them looked like disagreements and were one measurement under three courts.**
>
> The third: `classes.rs`'s pin asserts **81,599 B** and FG reports **81,312 B** for the same row.
> Both are correct. The test's own output line says which court it used — *"arity 2 **Merkle ids**
> … binding close 81599 B"* — and `kary_court(&bundle)` takes `dissection_arity` and
> `window_court_daa` from the bundle and **hardcodes `prompt_ids_form: MerkleV1`**. The shipped
> bundle cannot be Merkle: `validate_palw_v2` refuses to assemble one, because *"no writer or
> checker on this build reads it: every producer still commits the flat prompt-ids digest"*.
> **So 81,599 is a court the network refuses to run and 81,312 is the shipped figure.**
>
> *A helper that takes two of three fields from the real bundle and one from a literal produces a
> number that LOOKS bundle-derived and is not* — the same defect as pricing the cache-write route
> by hand, one field over, in a pin that passes and honestly prints the configuration nobody
> checked it against. `82,719 − 81,599 = 1,120` is exactly the
> arity-16 move disclosure, and the difference was already named in a test's own comment. **The
> genesis derives arity 2** from the v5 row in `genesis_objects`, on both the RC and devnet bundles
> — not the 4 a brief assumed, not the 16 an early sweep measured at. A byte count without its arity
> is a number from a court nobody is running.

**Every graph-v5 figure below is UNDER THE DISSECTION COURT.** This is not a footnote: under the
shipped BINARY court a fused leaf's terminal check opens the whole history it reads — about 1 MB at
n_ctx 512, twelve to thirteen carriers — which §5 shuts. So a graph-v5 row registered with only the
ladder and weightless fences armed is admitted and **unprosecutable**, and the number that says
otherwise was measured under a court the build would not be running. `palw_kary_court` is what
makes the table true, which is why §1 arms it.

**§3's central claim is now MEASURED, not argued** — priced at the ruleset's own 2^26 ladder on
`5f + impl(4ffb2b26)`, which is the first time the "precondition, not optimisation" line has had a
number under it:

    RC ceiling                        2,250,000 B = 27 chunks
    graph-v2/v3 @ 512                 1,999,729 B = 24 chunks   FITS
    graph-v5    @ 512, no dissection  3,446,708 B = 42 chunks   OVER THE CEILING
    graph-v5    @ 512, dissection     (FD's derivation)  4 chunks

So "a graph-v5 row registered with only the ladder and weightless fences armed is admitted and
unprosecutable" is **42 against 27** — refused at acceptance, by fifteen carriers. The dissection
court is what takes it to 4. That is the whole of §1's reason for arming `palw_kary_court`, in one
line, and it can now be re-derived by anyone rather than believed.

**Nor does the ladder bind either row — and here too v5 has the larger margin.** Worst-case step
leaf counts against the ruleset's 2^26, through `worst_case_step_leaf_count_capped_v1` at the
ruleset's ladder (not the un-suffixed form, which assumes 2^22 and refuses both):

    ruleset ladder cap            67,108,864
    graph-v2/v3 @ 512             59,000,848   FITS, margin 8,108,016   (12.1%)
    graph-v5    @ 512             52,778,128   FITS, margin 14,330,736  (21.4%)

`59,000,848` is the same number `palw-certify bind` printed independently for the v2 row, which is
two routes agreeing. The fused site removes leaves as well as kernels and carriers, so **every one
of the three ceilings this cut cares about — ladder, vectors, close — has more room under v5 than
under v2.** That is the fourth independent reason for the row we register, and none of them is the
fusion being interesting.

**The certification vector cap does not bind either row, and v5 is the cheaper of the two.** Measured
as an upper bound from the profile alone (`2 × |distinct (table, kernel) pairs|`, the 2 being prefill
and decode; the drill's real count can only be lower, since a leaf whose capture the material does
not hold is skipped):

    graph-v2/v3 @ 512   12 kernels, 16 (table,kernel) pairs, vectors <= 32   cap 32  FITS
    graph-v5    @ 512   10 kernels, 14 (table,kernel) pairs, vectors <= 28   cap 32  FITS

Fusing the four-node attention site into one **removes two distinct kernels**, so the row this
genesis registers needs fewer vectors than the graph-v2 row that sits exactly at the cap. An upper
bound is the right instrument here: at or below the cap it cannot bind, and only a number above
would have needed the full drill. So `PALW_CERTIFICATION_MAX_VECTORS` stays 32 and the hybrid's 74
is deferred with its refusal asserted by name — see §5.

*(That deferral is load-bearing rather than lazy: `PALW_CERTIFICATION_MASS_PER_VECTOR` is
`(CHUNK_MAX_BYTES × CHUNK_MAX_COUNT) / MAX_VECTORS`, so the per-vector price is DERIVED FROM THE
CAP. Raising the cap does not raise the fee — it lowers the price of each re-execution and leaves
the total identical, buying validators 2.31× the grading work for the same payment. Any future raise
must decouple the mass rule in the same change, or it is a fee cut wearing a bound's clothes.)*

**And the same table answers a real strategic question, which is why it is worth having.** A
graph-v2/v3 row at 512 **fits without ADR-0082 at all** — 24 chunks against 27, no fused site, no
court arming, and its 12 kernels are already covered by the shipped A16 family. So 0082 is not a
precondition for *a 512 class*; it is a precondition for *the v5 512 class*. The card said the
former and meant the latter, and the distinction is only visible once both rows are priced.

**We stay with graph-v5, and the reason is the close, not the fusion.** 24 carriers is a 0.3375 MSK
assembly deposit and a 192-DAA reserve on every close, against 4 carriers and 32 DAA — and that
reserve is charged to the court window whether or not anyone ever files, which is what compressed
the turn deadline in the first place. A v2 row would ship a class every honest dispute of which
costs six times what it needs to.

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
| graph-v5 dense @ 512, **dissection** court, **arity 16** | 80,504 | **94,500** | **1** | **yes** |
| graph-v5 dense @ 512, **dissection** court, **arity 2 — what genesis derives** | **81,599** | 95,595 | **1** | **yes** |
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

`dissection_arity` stays **2** on every preset until its fence arms. **The court's arity derivation
at the RC selects 2** — not the 4 an earlier draft of this card recorded, and not the ADR's worked
16. Verified by running on `palw-adr0082-impl` at `aa049f96`
(`the_rcs_derived_deadline_selects_an_arity_for_its_own_row_and_none_for_the_fences`):

| quantity | value |
|---|---|
| window / clock | 3,000 DAA / **42** |
| ladder | 2^26 |
| RC 512 row | arity **2** — 26 binary ladder rounds + 5 history rounds |
| moves | `2 × (26 + 5) + 2 + 1` = **65** |
| worst-case duration | 65 × 42 = **2,730** |
| assembly reserve | **216** (27 carriers) |
| total against the window | 2,730 + 216 = **2,946** < 3,000 |

The dense graph-v5 512 row — the row this genesis registers — derives arity **2** as well, at the
8-lane count it actually disputes. *The four numbers this table replaces (arity 4, 48 moves, 2,160
DAA, a 45 clock) were each correct when written and none of them survived FD; a figure is only as
current as the tree it was last read from.*

### Two derivations the cut deliberately does NOT take

Both were raised by fixer FD with the work already done and correct, and both are refused for the
cut. Recording them here because a derivation that exists and is not used looks like an oversight
to the next reader, and it is not one.

**M-5 — `court_max_close_bytes` keeps its literals.** FD built the derivation behind the assembler
(`palw_derived_court_max_close_bytes_v1`) and touched no shipped constant. Measured on `aa049f96`:

| set | derived | shipped |
|---|---|---|
| t11, graph-v2/v3 @ 512 | 2,250,000 B = 27 chunks | 2,250,000 B = 27 chunks |
| devnet, the shipped rows | widest close 78,688 B = 1 chunk → ceiling 83,333 B | 81,920 B = 1 chunk |
| graph-v5 @ 512, dissection court | 333,333 B = **4 chunks** | — |

The literals stay, for three reasons in order of weight:

1. `with_derived_court_close_v1` takes `families` and `rows` as arguments, and
   `PALW_LADDER_FAMILIES_V1` / `_V5` have **zero non-test callers** in the tree. An assembler that
   called it would have to hand it a families constant — the same free choice as a literal, one
   indirection deeper. That is priced-≠-pinned (ADR-0072 D8) wearing a derivation's clothes.
2. The ceiling is load-bearing in two OPPOSITE directions: `palw_mode_v2.rs:1211` refuses admission
   above it, and `palw_attn_court_v1.rs:1186` charges the assembly reserve from it. Tightening to
   the v5 pair's 4 chunks would buy ~184 DAA of window — the clock goes 42 → ~44, and both are
   ≥21× the hybrid's 2-DAA worst rung, so neither convicts an honest responder — and would
   **permanently refuse every class whose close exceeds 333,333 bytes on this network**. Admission
   is permissionless (ADR-0054). 5f is the first network to carry the dissection court; it is not
   also going to be the network that can only ever admit dissection-court shapes. That door is
   worth more than 2 DAA of move budget.
3. Devnet's 81,920 against a derived 83,333 is the same chunk count, so neither the reserve nor the
   clock moves. Only the literal would — a fingerprint move that buys nothing.

The builder and its reporting test stay in the tree, unwired: they are the falsifiable record that
the two numbers agree today and will say so when they stop. **After 5f**, the right shape is to
wire the derivation to `palw_shipped_court_rows_v1()` — the walk FD's own devnet arm already uses —
so its input is the registered set rather than a hand-supplied constant. Post-freeze, not at it.

**Decision 3's arity stays "smallest k that fits".** The alternative on the table was "maximise the
deadline", which would move the RC's clock from 42 to 48. Refused, and *not* because 42 is the
shipped number — reproducing 42 is the check, not the argument. Two reasons:

- The clock has ~21× headroom over the thing it must cover, while bytes-per-move has a hard carrier
  ceiling that arity 64 already breaches at the hybrid's 256-lane head
  (`palw_attn_dissect_arity_fits_carrier_v1`). Spend the abundant resource, conserve the scarce one.
- **"Maximise the deadline" is not a derivation.** It needs a second number — how much deadline is
  enough — and that number is chosen. Decision 3 is "the arity a ruleset DERIVES, never writes";
  the alternative reintroduces exactly the free field the rule exists to close.

*Revisit condition, so this is falsifiable rather than a preference:* if a measured replay floor for
any graph-v5 row ever comes within half the derived deadline, the trade flips and `k` should buy
rounds back.

---

## 4. The freeze and the single re-pin — ORDER MATTERS

**The agreed sequence, as of the last fixer. Every step's owner is named and every arrow is a
dependency, not a preference.**

    1  palw-merge-resolved 4205f535  -> impl     1c's three-way (5f + impl + the artifact branch)
    2  palw-launch-consolidated f7f56498 -> impl the four verifier fixes; ONE conflict,
                                                 host_security.rs, doc-only, pre-ruled in §3
    3  FH   the derived-artifact binding          on top of 2, because 2 already edits derive/src
    4  FG   the graph-v5 512 registration         profile/params only; merges impl at its end
    5  cargo +1.93.0 fmt --all                    ONCE, on the merged tree, LAST source change
    6  scripts/check-derive-freeze.sh             after 2 and again after 5 — until it prints nothing
    7  "frozen"                                   5b says it only after 6 prints nothing
    8  the re-pins                                mine, last, and nothing touches derive/src after

**Steps 3 and 4 are in that order for a reason that is not obvious.** FH writes in
`misaka-palw-derive/src/`, which `palw-launch-consolidated` also edits (`5e369d10`, `928b4081`,
`ce9a6a24`, `e2341aec`), so FH must land *after* the merge or it collides with four fixes it agrees
with. FG touches only the profile, params and test files, so it can come last and merge impl at its
end without racing anything.

**Step 6 is the only thing that makes step 7 sayable.** "Frozen" is a sweep, never a recollection —
see 3b below. It is run twice because step 5 is itself a change to the crate being frozen.


`transformer_id` is a function of `source_tree_sha256`, which covers **every byte** under
`misaka-palw-derive/src/` — comments included. It moves silently and everything stays green when it
does. A published derivation whose id no longer reproduces is unverifiable, so this is the last
thing done before the cut and it is done ONCE.

**Order:**
1. Freeze `misaka-palw-derive/src/` completely. Three doc edits and one formatting pass are already
   inside the hashed bytes.
2. Run `cargo +<pinned> fmt --all` LAST among source-changing steps — formatting is inside the hash.
3. Then re-pin `transformer_id_pin` and `shipped_presets_have_pinned_fingerprints`, in one commit.
3b. **"Frozen" is a sweep, not a recollection** — `scripts/check-derive-freeze.sh`. Re-run it after
   every merge until it prints nothing. As of this writing **four** branches still move the crate:

       MOVES IT   palw-artifact-names-genesis-row  -> 10b7eac25bab
       MOVES IT   palw-launch-consolidated         -> defa73b60943
       MOVES IT   palw-launch-derived-proof        -> 79305dbfe5f1
       MOVES IT   palw-launch-qwen36-demo          -> 3f5996e60331

   **Three sweeps were written for this and only the third asks the right question**, which is worth
   knowing because the first two look authoritative and disagree:

   | sweep | asks | answer |
   |---|---|---|
   | `git diff <rel> <b> -- <path>` | do the END STATES differ | **11** — mostly abandoned branches that merely predate the work |
   | `git log <rel>..<b> -- <path>` | are there COMMITS touching it | **23** — includes `palw-adr0082-impl`, whose `derive/src` is byte-identical to 5f's |
   | merge-tree, compare the subtree hash | would the MERGE move it | **4** — the only count that predicts a stale pin |

   All three are honest counts of different things. The pin depends on the third.

   *This step exists because the pin was taken early and reverted.* The claim was "the merge cannot
   move `transformer_id`, checked rather than assumed", and what had been checked was
   `palw-adr0082-impl` and two in-flight fixers. **That claim about impl was true** — impl really
   does not move the tree. What was false was generalising it to the merge, when three launch
   branches and 1c's do. Two sessions reached the same wrong shape independently and by the same
   route: measure the thing in front of you, assert the conclusion for the population.

4. Nothing under `misaka-palw-derive/src/` may be touched after step 3, for any reason, including a
   typo in a comment.

**The re-pins, by owner. Nothing touches both sides.**

| pin | where | owner |
|---|---|---|
| `transformer_id_pin` | `misaka-palw-derive/tests/` | here, at the cut |
| `shipped_presets_have_pinned_fingerprints` | `consensus/core/src/config/params.rs` | here, at the cut |
| `golden_vector_ids_are_frozen` | `consensus/core/src/palw_freeprompt_v3.rs` — ADR-0082 stream H gave the job two fields | here, at the cut |
| **genesis `utxo_commitment`** | `all_networks_genesis_constants_match_premine` | here, **FIRST OF ALL** — via the `config::premine` CEREMONY tool, not a hand edit |
| `PALW_RC_COURT_E2E_ROOT_BYTES` | `consensus/core/src/palw_e2e_adjudicability.rs` | here, second — see the ordering below |
| state version 18 → 19, ADR-0043 goldens | `palw-adr0082-impl` | 5b, on that branch |

### A RED PIN PROTECTS NOTHING WHILE IT IS RED — the re-pin must be a PREDICTION, not a reading

**This changes how the ceremony is run and it is the most important procedural finding of the cut.**

A re-genesis is precisely the window in which several pins are red at once. While they are red they
are not guarding anything, so **a second change made in that window is invisible — and then gets
blessed by the paste.**

Proved by mutation rather than argued. One byte of `PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT` flipped
(`0xf4` → `0xf5`), consensus-core run before and after:

    clean tree    3 failed:  premine, fingerprints, freeprompt golden
    mutated tree  3 failed:  premine, fingerprints, freeprompt golden      IDENTICAL

    but the fingerprint test's own message moved:
      clean    testnet-11 … got f38049f023cec8d4…
      mutated  testnet-11 … got 1d03bd9c143f39f1…

**The gate detects the change and cannot report it.** An operator watching pass/fail sees three reds
before and three reds after, with the same three names. Whoever then re-pins by pasting the `got`
value pastes the mutated one and **freezes the accidental byte into the genesis** — after which
every gate is green and the tree is wrong.

**So the re-pin is a prediction that must be confirmed, never a value that is read and pasted:**

    1  BEFORE any further change, on the frozen tree, record the expected fingerprints.
    2  Perform the re-pins.
    3  The value the ceremony reads MUST equal the value predicted in step 1.
    4  If it does not, NAME the change that moved it — or stop. "It moved" is not a finding;
       "it moved and here is why" is. An unexplained move in this window is the one thing
       this window cannot otherwise show you.

*The method is the deliverable, not any particular string.* Both prediction values must be taken
from the **frozen** tree by whoever holds it — FG moves testnet-11's legitimately, so a prediction
taken before FG lands is a prediction for a tree nobody ships.

*Found by 1c, who built a scanner for this class, noticed its baseline was generated from the tree
it checks and therefore could not fail, and asked the underlying question by hand instead.*

### THE RE-PIN LIST WAS WRONG AND THE MISSING ONE ABORTS EVERY HOST AT STARTUP

**`PALW_RC_GENESIS` holds TWO literals that both move at this re-genesis, and the procedure re-pins
one of them.** `hash` (`config/genesis.rs:348`) and `utxo_commitment` are separate constants; the
header hash covers the commitment, so the community premine moves both.

Every guard that could catch the second one is looking somewhere else:

| guard | what it actually covers |
|---|---|
| `every_genesis_commits_to_the_premine_this_build_mints` | **`utxo_commitment` only** — and it DOES name `PALW_RC_GENESIS` (`genesis.rs:469`). Goes green on a commitment-only paste. |
| `config::premine::tests::print_premine_commitment` — the printer the FAILURE MESSAGE names | printed `*_PREMINE_UTXO_COMMITMENT` and `*.utxo_commitment` lines. **Never a genesis hash.** Fixed at `68f0a1b6`. |
| `repin::print_repinned_rc_genesis` — the printer the RUNBOOK names | **always printed both**: it assigns the new commitment into `params.genesis` and *then* derives the header, so `REPIN hash` is the hash the new commitment implies. Never had the gap. |
| `test_genesis_hashes` — the test that recomputes each genesis' own hash | iterates `[GENESIS, TESTNET_GENESIS, TESTNET11_GENESIS, SIMNET_GENESIS, DEVNET_GENESIS]` (`genesis.rs:499`). **`PALW_RC_GENESIS` is not in that list.** |

**The precise shape is worse than "an unchecked constant", and the difference matters.**
`PALW_RC_GENESIS` is *not* unwatched: `every_genesis_commits_to_the_premine_this_build_mints` names
it directly. What is unwatched is **two of its fields** — `hash` and `hash_merkle_root` — which are
exactly the two that abort a host at startup. **The one test that names the constant covers the
field that is not the problem**, so the constant looks guarded, and a reader checking "is this
pinned value tested?" finds a test, sees the name, and stops.

And `TESTNET11_GENESIS` is not a near-miss for it — `genesis.rs:462` calls it a **"retired fossil"**,
`params.rs:7969` sets `params.genesis = PALW_RC_GENESIS`, and their hashes differ
(`08e9c8a4…` against `572f80c0…`). **So the hash test recomputes the genesis this network does not
ship and skips the one it does.**

**The failure sequence, which is why this is the worst thing found today:** the operator adds the
community entries, runs the first gate in §6's own table, gets the premine failure, runs the printer
it names, pastes what it prints — and `cargo test -p kaspa-consensus-core --lib` goes **fully
green** while `PALW_RC_GENESIS.hash` still names the pre-community block. The fleet is then stopped,
wiped and redeployed, and **every host aborts at startup**. There is no earlier symptom, because
every gate the card told the operator to run has passed.

**So the freeze re-pins FIVE, not four**, and the fifth has no test until one is written:

    1  PALW_RC_GENESIS.utxo_commitment    every_genesis_commits_to_the_premine_this_build_mints
    1b PALW_RC_GENESIS.hash               test_genesis_hashes  (CLOSED — see below)
    2  the two shipped fingerprints       shipped_presets_have_pinned_fingerprints
    3  the free-prompt golden             palw_freeprompt_v3::golden_vector_ids_are_frozen
    4  the eight transformer ids          transformer_id_pin

**CLOSED on impl at `68f0a1b6`, before the ceremony rather than after.** Both one-liners landed:
`PALW_RC_GENESIS` is now the sixth entry in `test_genesis_hashes`'s array, and
`print_premine_commitment` prints, beside the commitment, the `PALW_RC_GENESIS.hash` that commitment
implies (and the `hash_merkle_root` it does not move).

**The new guard is GREEN today, and that is the honest reading rather than a weakness.** The stored
hash covers the stored commitment, so the struct is self-consistent right now; it goes red *the
instant the commitment is re-pinned without the hash*, which is the only moment it needs to fire.
A guard that were red today would be telling you about the old genesis.

**Take the value from the cut's own commit, not from here.** The printer's output moves when any
genesis object moves — FG's v5 registration will move it — so a hash printed before FG lands is a
hash for a genesis nobody ships. Run `config::premine::tests::print_premine_commitment` on the
frozen tree and paste what it prints, both lines.

**There are TWO ceremony printers, and the defect was which one the failure message pointed at.**
`repin::print_repinned_rc_genesis` (kaspa-consensus, named by the runbook) has always printed both
values, correctly — it substitutes the new commitment into `params.genesis` before deriving the
header. `print_premine_commitment` (kaspa-consensus-core, named by the *test failure message*)
printed commitments only, until `68f0a1b6`.

So an operator following the **runbook** was safe; one following the **failure message the first
gate prints at them** was not. That is the live-site-versus-search-order defect exactly: the message
wrote an address, and the tool at that address was the incomplete one. Both print both now, and both
derive the hash the same way, so they cannot disagree.

*I first recorded this as "the ceremony tool does not print the value", which was true of one of two
tools. Corrected after reading the runbook's printer in full rather than a grep of selected lines
from it — the assignment that made it correct was on a line my pattern did not match.*

### The re-pin list is SEVEN items and only FOUR have a red test — here is where the other three went

Measured on the re-merged tree (`4205f535`): `misaka-palw-base0` 360/0, `kaspa-consensus-core`
1815/3, `misaka-palw-derive` 201/1. **Four reds, and each is one of the seven:**

    every_genesis_commits_to_the_premine_this_build_mints    re-pin 1  premine utxo_commitment
    shipped_presets_have_pinned_fingerprints                 re-pin 3  the two fingerprints
    palw_freeprompt_v3::golden_vector_ids_are_frozen         re-pin 5  free-prompt golden
    transformer_id_pin::…_pinned_with                        re-pin 4  the eight transformer ids

The other three are accounted for and none of them is a silent hole:

- **re-pin 6, the state-version goldens: already done, by FA2.** `PALW_STATE_V2_VERSION` is 20 and
  the empty/inhabited goldens were re-pinned in the same change that bumped it
  (`966bae07…` / `a0b711e1…`, in `palw_state_v2.rs`). A version bump that re-pins its own goldens is
  the right shape; nothing is left for the freeze.
- **re-pin 2, `PALW_RC_COURT_E2E_ROOT_BYTES`: gated, and currently NOT stale.** The gate is real and
  it is not in consensus-core — `the_pinned_rc_e2e_root_is_what_this_build_certifies`
  (`misaka-palw-base0/src/e2e_drill.rs:1745`), a plain `#[test]`, which compares
  `palw_court_e2e_root_v1()` (computed from the registered families) against the pin. base0 is
  360/0, so on this tree the root has not moved. The `581466da… → 8f08d303…` move measured elsewhere
  was against a per-kernel drill rule that is **not merged**; if it lands, this becomes a live re-pin
  and base0 goes red to say so.
- **re-pin 7 does not exist** — the seventh slot was the two fingerprints counted as two items.

**And one assertion in this area checks nothing, with a confident message.** `config/params.rs:11228`:

    assert_eq!(bundle.court_e2e_root, palw_rc_court_e2e_root_v1(),
               "the attempt-lane genesis set is the one this build certifies");

`bundle.court_e2e_root` is **set from that same function** (`palw_fp_devnet_v3.rs:803`). It compares
a value with itself. The thing its message claims — that this build's court can play the family set
the pin names — is `palw_court_e2e_root_v1()`, a different function that this test never calls.
It is the `signed: true` defect again: *a word answering a different question than the one it was
asked.* The real check exists in base0; this line should say what it actually checks (that the
assembler does not let a caller name the root) or point at the gate that does.

### The certified-set root is regenerated TWICE, and that is correct

It is not a freeze-time chore. With the covering type change in, **no node can register a class at
all**: the registration pricing path compares the supplied certified set against the network's
commitment before it prices anything, so a stale root stops an acceptance drill at stage 2 and
nothing downstream can be measured. Found by running a chain, not by a suite.

    now, on the owning branch    provisional, tree sha in the commit message, so the drill can measure
    at the freeze, here          once, from the final tree

**The pair is a measurement of the interval between them.** If the freeze's value matches the
provisional one, nothing that landed in between touched a family digest; if it differs, the
difference names exactly what did. Two regenerations handle staleness better than one at a moment
nobody can identify in advance — and there is no re-pin hazard here, because the hazard in a re-pin
is JUDGEMENT and a derived value carries none.

**The regeneration must come from the DRILL, not from the profile.** Writing
`drilled_kernel_ids = profile.reachable_kernel_ids_v1()` would compile, would be green, and would be
a set that was typed rather than drilled — the exact defect the field exists to close. If the two
turn out equal, that is a result to be observed, not a shortcut to be taken.

### A tautology is not a stale claim — the doc was never true, and the check was built not to notice

`the_shipped_rows_keep_interval_one_and_their_ids` asserted `qwen25_a16_class_id_v2() ==
qwen25_a16_class_id_v2()` — a pure function compared with itself, whose failure message was
unreachable. Writing the comparison its own doc describes turned it red and produced **three class
ids for one model at one width**:

    f942e268…   qwen25_a16_profile_v1(QWEN25_1_5B_A16)       the superseded v1 graph
    71bbb755…   qwen25_a16_profile_v2(QWEN25_1_5B_A16)       what the genesis REGISTERS
    7a76d29b…   palw_a16_context_row_profile_v1(16)          that module's own projection

"Three adjacent ids loose in this project's notes" has been a warning in these commit messages for
days. Here they are, measured, and **the reason nobody found them is that the only assertion in the
area could not fail.**

**This is a different defect from a stale claim and the difference is worth holding.** A stale doc is
a true statement that expired. This doc was **never true** — so the only assertion that could sit
underneath it was one incapable of failing, and the test was shaped to agree with the prose rather
than with the code. You cannot find it by re-reading either the doc or the test; you find it only by
**writing the comparison the doc describes and watching it go red**.

**And the shape is NOT mechanisable — this is a limitation, measured, not an unwritten gate.**
`assert_eq!(f(), f())` looks greppable, so it was swept: **25 hits, about 20 of them legitimate**
determinism checks where `f` has hidden state (fresh buffers, iteration order, memoisation) —
`the_same_dsl_twice_is_the_same_bytes` under a "determinism" heading, `assert_eq!(run(), run())`
where `run()` builds fresh state each call, and eighteen more. **A gate on this pattern fires on all
of them, and the first thing anyone does is add allow-comments to the real determinism tests** —
which leaves the tree worse than no gate, because the exemptions become where defects hide and now
they carry a blessing.

What made the defect a defect was never the shape: it was **the doc above it claiming a different
comparison**. Had that doc said "this id is stable across two derivations", the identical line would
have been correct and unremarkable. *The discriminator is agreement between prose and assertion, and
no static check reads that.* The instrument stays manual: **do what the doc says and see whether the
code minds.**

**The root cause is worth more than either.** All three instances were written in the same sitting
as the thing they test — *the nearest value is the one you already have in hand, and it type-checks*.
That IS greppable, unlike the shape: **a test introduced in the same commit as its subject** is where
to look, as a review heuristic rather than a gate.

*Pinned as three-way DISTINCTNESS rather than three literals — a literal would join the re-pin set
whenever any of the three graphs moved, for no benefit, while the property actually worth holding is
that they stay tellable apart. A collision means a row built by one route silently names the class
another route owns, which is the `bind` defect in a different costume.*

### The dense lane is broken from BOTH ends, and it is ONE fixer — do not land half of it

Two findings that read as separate items are the same defect seen from two sides, and FG closes both:

    the class the chain HOLDS cannot be executed   registered row declares rms_eps_q 1;
                                                   every artifact carries eps_q 256, so
                                                   A16Engine::plan_from_profile refuses it —
                                                   while that row holds 489‰ of cadence
    the class the WORKER embodies is not held      palw-a16-fp-worker's MODEL_ID is the v5 512
                                                   row, which the genesis does not register, so a
                                                   commitment under it names a class no chain holds

**Registering the v5 row and arming `palw_kary_court` were never separable, and neither are these.**
Landing the registration without the eps replacement leaves 489‰ on an unexecutable class; landing
the eps fix without the registration leaves the worker pointed at nothing. *Written as one item
because two items are how half of it ships.*

### THE 62 SKIPPED TESTS INCLUDE EVERY TEST THIS NIGHT EXERCISES HARDEST

The full suite is **4,072 run, 4,066 passed, 6 failed, 62 skipped** in 407s — and the six are the
five re-pin tests plus one fixture branch, exactly as owned. But the **skipped** set is the one worth
reading, because of what this particular night is:

    ibd_participation_tests::a_node_killed_partway_through_recovery_comes_back_safe
    ibd_participation_tests::e2e_a_a_stronger_chain_found_during_ibd_wins
    ibd_participation_tests::e2e_b_bootstrap_recovery_crosses_a_provisional_pruning_point
    ibd_participation_tests::mainnet_gate_handoff_holds_repeatedly_over_a_delayed_link
    ibd_participation_tests::mainnet_soak_randomized_fault_injection
    daemon_integration_tests::daemon_utxos_propagation_test
    simpa tests::test_pruning_via_simpa
    palw_agent_equivalence::the_resident_agent_and_a_fresh_process_compute_the_same_tag

**These are `#[ignore]`d for cost, which is a defensible standing decision and a DIFFERENT decision
from "ignored on the night every host is wiped and every node re-syncs from a new genesis."**

The isolated boot on ibm covers the **start**. Every test above covers *a second node joining a
chain that already moved* — which is literally the next thing that happens after the producer comes
up. And this project has been bitten there: a pruned-IBD panic that put a joining node into
permanent quarantine.

*A standing decision made for cost becomes a different decision on the one night its subject is the
main event. The set did not change; the night did.*

**And that is a THIRD member of the staleness family, more invisible than the other two:**

| what goes stale | what pushes back |
|---|---|
| a claim that **broke** | a red test, a failed build, a confused reader — *something* |
| a claim that **improved** | nothing. It reads as a correct caution (see below) |
| a **decision** whose premise stopped applying | nothing, and **it was never a statement at all** |

The third is the worst because *a standing decision does not come up for re-decision* — it persists,
correctly, into a night with different physics. `#[ignore]` for cost is right on an ordinary week and
nobody re-decides it, because nothing in the mechanism asks. **There is no sentence to re-read.**

*The filter that finds them is not "what claims might be stale" but "what does THIS operation
uniquely create". A resident agent disagreeing with a fresh process can only surface after every host
restarts at once — nothing in the normal running of a chain produces that condition, and a wipe
produces it on every machine simultaneously.* Six of the ignored tests are being run on that
reasoning: the three IBD-participation cases, and the three pow-driver ones — including
`a_dead_agent_costs_a_delay_and_not_a_tag`, because **a wipe is every agent dying at once.**

**RUN, AND GREEN — on the cut tree, with the duration proving it ran.**

    impl 41e364b6
    ibd_participation_tests::a_node_killed_partway_through_recovery_comes_back_safe
      RESTART complete: 0 failures out of 7
      test result: ok. 1 passed; 0 failed  —  242.35s

**242 seconds and seven restart scenarios**, against the 0.016s that would have meant a skipped
fixture. So the risk this section states is now *measured on this tree* rather than enumerated: a
node killed partway through recovery comes back safe, which is the failure with the worst story if
it bit — an operator being told what to delete on a chain nobody has a mental model of yet.

*One of eight. The other seven remain a stated risk, and three IBD cases plus three agent-recovery
cases are running as this is written. `mainnet_soak_randomized_fault_injection`,
`mainnet_gate_handoff_holds_repeatedly_over_a_delayed_link`, `daemon_utxos_propagation_test` and
`test_pruning_via_simpa` are not being run and that is a stated gap, not an oversight.*

**Superseded note — the reasoning that got us here:** — it is the
failure mode with the worst recovery story, and the wall clock is free while the last fixer
finishes. Whatever it prints goes in §6 as a gate that was run rather than a set that was skipped.

### THE PREDICTION, taken pre-fmt on impl `41e364b6` — check every paste against it

Recorded **before** the formatting pass, per the ceremony rule, so a value that moves without a named
cause stops the paste. `derive/src` tree at capture: `4969f8dc051cac31`.

    1  premine    PALW_RC_GENESIS.utxo_commitment
                    pinned  2d882275dae82945a99e825fcb5f973c66a9e945f4d0849d833e8b8f9c0835ff…
                    builds  ba2612417e7e0817cca0ac0cade91caa585834c051114bc4125542acb05898db…
                  and .hash, printed beside it by the ceremony tool

    2  fingerprints  testnet-11  a7baab7957d27bbd… -> 71efa66480211731e3dc6fa2312ed73f
                                                      7ed11b93372a19a55ac66ef39b65920e
                    devnet      84153175ce880504… -> c0da0c9024d68b94b95010d1566cb1d5
                                                      35a818cd0727d9978906b0a2a8b13692

    3  free-prompt golden   pinned 700b90364860460f7d89d85eed59019c
                            new    c940b5c36ee40846087e6c5927d6e6b5

    4  the eight transformer ids + source_tree_sha256 — THE ONLY ONES THAT MOVE AT THE FMT

**Three of the four must be UNCHANGED after the fmt**, because formatting touches no consensus
input. If any of 1–3 differs from the above when the ceremony runs, something landed between this
capture and the paste, and the ceremony stops until it is named.

**The two fingerprints were measured independently by two sessions and agree exactly** — `71efa664…`
and `c0da0c90…` from 5b's run and from mine, on the same tree by different invocations. That is the
two-ways rule applied to the prediction itself, which is the one number a wrong paste would freeze
into the genesis.

### RED-COUNT IS NOT RE-PIN-COUNT, and two tests print left/right in opposite senses

**Five re-pin tests. Four re-pin values.** Anyone checking their work by "did the number of reds go
down by one" after each paste will over-count and go looking for a fifth value that does not exist.

The premine value alone accounts for **three** observations, all of the same pair:

    kaspa-consensus       utxo_set_override::all_networks_genesis_constants_match_premine
                          left  ba2612417e7e0817…  (BUILT)     right 2d882275…  (PINNED)
    kaspa-consensus-core  every_genesis_commits_to_the_premine_this_build_mints
                          "pinned 2d882275…, premine BUILDS ba2612417e7e0817…"
    the isolated boot     utxo_set_override.rs:60 — the node refuses to start on the same pair

**And the two tests print the pair in OPPOSITE senses.** One puts the built value on the left, the
other names the pinned one first. A reader holding both messages side by side sees `ba26…` on the
left in one and on the right in the other, and the natural reading is *the two tests disagree*.
**They agree exactly.** That is a five-second panic at the worst possible moment and it is free to
know about now.

*Both are the same underlying thing: a count and a sense are properties of the REPORTING, not of the
work, and at 3am the reporting is all anyone has.*

### THREE REDS CLOSE WITH ONE PASTE — and one gate breaks the script's own rule

Full `ci-gates.sh` on the merged content: **15 passed, 4 FAILED**, and every failure is a re-pin this
ceremony performs or the fmt sweep. **No gate fails for a reason this cut does not already own.**

    artifact-stranger   FAILED rc=2     transformer re-pin  \
    derive-suite        FAILED rc=101   transformer re-pin   > ONE file: transformer_id_pin.rs
    (consensus-core)    the pin test                        /
    nextest             FAILED rc=100   premine re-pin
    fmt                 FAILED rc=1     43 inherited diffs, closed by the single pass

`transformer_id_pin.rs` closes **three** of them — a consensus-core test, a derive-suite gate and a
CI gate — because the stranger *reads its pins out of that file* rather than restating them. That is
the one-spelling rule paying a dividend at the freeze: one paste, three reds.

**And `nextest` breaks the script's own rule 2.** `ci-gates.sh` says *"a checker that prints its
verdict without printing its coverage is unfalsifiable"* — and `nextest` fail-fasts at **34 of
4,072**, so its verdict is *the first failure* and never *the set*. It prints the coverage number
and nobody reads it as one. **The gate script states the rule and one of its gates violates it**,
which is the postmortem-next-to-its-own-instance shape, in the file that exists to enforce the rule.

*After the cut, not now:* `--no-fail-fast` costs wall clock on a red run and buys the entire red set
on the run where it matters most, which is exactly a freeze. Changing a gate's invocation during one
is how a freeze acquires an unmeasured variable.

### FAVOURABLE staleness is the kind that survives — nobody re-checks an item that got better

Three claims in these documents went stale today and **all three had improved**:

    the hybrid's exclusion       "cannot close at ANY width"  ->  3 carriers against a ceiling of 27
    the width table              "capped at 39 positions"     ->  the ladder moved to 2^26
    the announcement's Known-open "the hybrid is not registered" -> FG registered it

**Not one of them was a claim that had broken. Every one was a constraint that had lifted.** A claim
that breaks produces a red test, a failed build, a confused reader — something pushes back. A
constraint that lifts produces nothing at all, and the sentence describing it keeps reading as
true, in a document nobody has a reason to revisit. *An item that got better is one nobody is
motivated to re-check.*

So the freeze pass has a specific instruction, not a general one: **go through every "known open",
every "cannot", every "not yet" and ask what would have to have changed for it to be false — then
check whether it did.** That is a different sweep from looking for broken claims, it takes minutes,
and it is the one that catches the sentences a re-read cannot.

*All three of these were caught by grepping for the claim, not by re-reading the document. Re-reading
is what let them survive: a favourable staleness reads exactly like a correct caution.*

### ASK THE SAME THING TWO WAYS — the only cheap detector for a broken instrument

**This is the method that made this cut work, and it was used all day before anyone named it.** Every
pair below caught something; **not one of the single measurements would have.**

| the pair | what the second one caught |
|---|---|
| ancestry **and** content-grep | a fixer "absent by ancestry" that was present by content, and vice versa |
| arity **and** prompt-ids form | 81,599 measured under a court `validate_palw_v2` refuses to assemble |
| the library route **and** the binary route | `bind` naming the genesis row in a test and the wrong graph through its own argument parsing |
| a re-armed ruleset **and** the genesis object itself | a third close figure, from the only route that asks what the chain answers |
| the pass count **and** the wall clock | a suite whose artifact cases skipped, printing the same ten dots and the same `ok` |
| two sessions measuring independently | every number in this card that survived |

**The mechanism is not redundancy — it is that a broken instrument answers ONE question
confidently.** A failed fetch, a filter with no digit class, a helper hardcoding one of three fields,
a skip that returns early: each produces a believable result, and nothing inside that result says it
is wrong. **A contradiction between two askings is the cheapest signal in this whole system**, and it
is available for the cost of asking twice.

*The counter-instinct is the dangerous one: when two measurements disagree, the pull is to pick the
convenient one and move. Every finding in this section came from someone refusing to.*

### The short function name is the genesis-anchored one, and it answers the wrong court

**Measured against myself: I reached for the wrong variant six times in one day**, and every time the
wrong answer looked like a finding rather than a mistake. The pattern is mechanical enough to state:

| the short name | what it silently assumes | the one that asks the ruleset |
|---|---|---|
| `derive_court_cost_v1(profile)` | `PALW_STEP_MAX_LEAVES` = 2^22, `kv_checkpoint_bytes: 0`, `dissection: None` — the cache-write route | `derive_court_cost_shaped_v1(profile, shape)` |
| `worst_case_step_leaf_count_v1(profile)` | 2^22 | `worst_case_step_leaf_count_capped_v1(profile, ladder)` |
| `step_merkle_root(leaves)` and its five siblings | `PALW_STEP_LEG_MAX_LEAVES` | the `_capped_v1` form |
| `PalwCourtCostShapeV1::genesis_anchored_v1` | the genesis ladder, not the ruleset's | build the shape from the ruleset's rules |

**Every one of these is correct for its own purpose and wrong as a measurement of the shipped
network.** The failures they produced today, in order: both dense rows "refused at 512" (they were
priced at 2^22); the 512 close "arity-invariant" (priced on a route the ruleset does not play, where
the binding node is not the fused one and `shape.dissection` is read only for `AttnFused`); and
every A16 row "REFUSED at 512" a second time from the leaf count.

*The tell is a result that is identical across inputs that should change it, or a refusal whose
`max` is a number the ruleset does not use.* `classes.rs` states the rule for its own case — "a test
that swapped the route by hand would be pricing a route the ruleset no longer plays" — and the
general form is: **the convenience wrapper is the easiest to call and it answers a different court.**

### The tell for "is this a chain change?"

**A change that moves a value the chain commits to is a chain change, and its cost is measured on a
chain.** But the useful half is the tell, because knowing the rule did not prevent this one:

> **It is not "did something move". It is "does anything OUTSIDE this crate compare it to
> something".**

The value that moved was nameable — `palw_court_e2e_root_v1` — and the cost was still reported in
tests, because the comparison that makes it expensive lives in the registration pricing path against
a genesis commitment, and one side of that comparison is a chain. No suite can hold it.

**The premine commitment is re-pinned before everything, because it is what makes this a
re-genesis.** `premine_is_the_expected_split` PASSES — the premine itself is right and only the
commitment pin is stale — but that pin is the genesis UTXO set's identity, so the genesis hash and
every value downstream of it move with it. It is also the only one of the seven that needs a
ceremony tool rather than an edit.

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

**Careful with the third**: `golden_vector_ids_are_frozen` exists TWICE — `palw_freeprompt_v3.rs:1643`
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

### The merge surface, measured ahead of the freeze — FIVE conflicts, one already decided

Trial-merged in a throwaway worktree so the conflict set is known before the cut rather than
discovered during it.

**The first version of this section said "one conflict" and was wrong, from two correct
measurements.** Each branch conflicts with 5f in exactly one file — and that says nothing about the
merge actually being done, because the remaining four conflicts are between the two BRANCHES, and
neither pairwise merge contains both:

    5f + palw-adr0082-impl                 -> 1   misaka-palw-base0/src/fuzz_a16.rs
    5f + palw-artifact-names-genesis-row   -> 1   misaka-palw-base0/src/fuzz_a16.rs

    5f + impl, THEN + artifact-names       -> 5   fuzz_a16.rs (as above), plus
                                                 consensus/core/src/palw_class_admission_v2.rs
                                                 consensus/core/src/palw_state_v2.rs
                                                 misaka-palw-base0/src/classes.rs
                                                 misaka-palw-base0/src/fp_recompute.rs

Order does not help: five either way. *`5f + A` and `5f + B` do not predict `5f + A + B`* — and the
whole point of a trial merge is to trial the merge you are going to do, which this card asserted
while not doing.

**A merge surface is a measurement with a timestamp.** An earlier three-way run had
`palw-a16-fp-worker.rs` in the set and today's does not; 5f moved thirty-odd commits between them.
This set is against `2af2e5c5` and will move again with the next fixer. Re-run before the cut; the
count is not the deliverable, the *rehearsed resolution* is.

**The conflict, and its resolution, are already ruled.** 5f has `pub fn next(&mut self) -> u64` with
an `#[allow(clippy::should_implement_trait)]`; impl and 1c's branch both have `pub fn next_u64` with
no allow. **Take `next_u64`.** The justification written beside that `allow` was false — it claimed
`next` is what every RNG calls this step, in a file that already spelled it both ways twice over,
and `rand_core`'s own name for it is `next_u64`. The half of that doc comment worth keeping is the
Iterator argument, which is true: this is an infinite deterministic sequence, and implementing
`Iterator` would offer combinators that are meaningless on it and let a `for` loop run forever while
reading as ordinary code. Keep that sentence, drop the lint apologia, take the rename.

*Recording it here because a one-line conflict resolved under time pressure at a cut is exactly how
a retracted argument gets re-adopted: the `#[allow]` side reads as the more thoroughly justified one,
and it is the wrong one.*

**A THIRD merge is required and the card did not name it: `palw-launch-consolidated`.** It is not
optional polish — it carries two defects of exactly the class this cut keeps finding, in the tool an
operator uses to verify a published derivation:

    5e369d10  "I cannot check this" was being reported as "this is a forgery"
              UnknownGrammar / UnknownTransformer -> UNVERIFIABLE, still exit 2
    928b4081  `"signed": true` was a BORSH field being PRESENT, not a signature that verifies
              it was `signature.is_some()`; a signature that is a byte of noise verified `consistent`
    ce9a6a24  the drill silently skipped contract/evm/v1 and said nothing
    e2341aec  `width` — the row's arithmetic, so a narrow class refuses BY NAME

`signature.is_some()` printed as `signed: true` is the sharpest of the four: the tool that exists to
let a stranger check a derivation was answering a different question than the word it printed.

**One merge, not three** — `palw-launch-consolidated` already contains `palw-launch-derived-proof`
and `palw-launch-qwen36-demo` (verified by ancestry). It adds 35 commits over the re-merged tree and
conflicts in exactly one file.

**That conflict is pre-ruled: `misaka-palw/src/host_security.rs`, and it is DOC-ONLY.** Two hunks,
zero non-doc lines inside either. Both sides independently discovered the same defect — that
`MISAKA_PALW_NETWORK_ID` missing from `PALW_WORKER_ENV_ALLOWLIST` made every free-prompt worker
unstartable, with SA-7 withholding the real reason so the gateway reported only "the worker exited
before announcing its manifest" — and both wrote it up. The allowlist entry itself is byte-identical
on both sides at line 78.

    KEEP HEAD's test.  HEAD has `assert!(PALW_WORKER_ENV_ALLOWLIST.contains(&"MISAKA_PALW_NETWORK_ID"))`
                       at line 949; consolidated has no such test. Losing it is the real risk here.
    MERGE the prose.   HEAD explains the two well-founded rules that pointed opposite ways and that
                       a name is not a capability; consolidated has the reproduction (`env -i` with
                       the exact delivered set) and the exact `die` message. Both halves are worth
                       keeping and neither is a superset.

*Two sessions finding the same defect and writing two explanations is the cheapest kind of conflict
and the easiest to resolve badly: a side-pick keeps one write-up and silently drops a test.*

**The other four, and who owns them.** `classes.rs`, `palw_class_admission_v2.rs` and
`palw_state_v2.rs` are where the fourth certified family, the artifact-route change and the
`canonical_leaves` field meet ADR-0082's court — 1c's to resolve, and they are resolving them.
`fp_recompute.rs` is impl-against-J and is mine. **Verified state of the two-way merge**, so the
three-way starts from something known: `misaka-palw-base0` 327 passed / 0 failed (the crate the
`fuzz_a16` resolution is in), `kaspa-consensus-core` 1,808 passed / **3 failed, and all three are
re-pins this cut performs anyway**:

    config::genesis::tests::every_genesis_commits_to_the_premine_this_build_mints   re-pin 1
    config::params::…::shipped_presets_have_pinned_fingerprints                     re-pin 3
    palw_freeprompt_v3::tests::golden_vector_ids_are_frozen                         re-pin 6

No fourth red, and no red that is not on the re-pin list. That is the result worth having from a
trial merge — not the conflict count, which expires.

---

### The artifact binding, verified by running — and the WALL CLOCK is what proves it ran

FH closes the hole the audit found. Verified here, not read: `misaka-palw-derive/tests/answer_binding.rs`,
**10 passed / 0 failed / 0 ignored / 0 filtered out, finished in 18.40s**, with
`instruct-bound.palwart` (1,795,427,276 B) at the path the fixtures name and `qwen25-tokenizer.json`
where they expect it.

**AND `--run-ignored all` IS THE MIRROR: the same missing fixture, the opposite verdict.**

    a skip that returns early         counts as PASSED
    an ignored test FORCED to run     counts as FAILED
    same absent fixture — and NEITHER verdict is about the code

Measured: the three `palw_agent_recovery` tests "failed" in **0.016s** each under `--run-ignored
all`, panicking on `env::var("MISAKA_PALW_GGUF").expect(…)`. Nothing ran. **The duration is the tell
again** — 16 ms cannot spawn a resident agent and a fresh process.

**The `#[ignore]` reason string was the only honest part of the mechanism** — *"needs the real
palw-worker and the 1.2 GB pinned model"* names exactly what is absent — and forcing the run
discards it. *Reaching for more coverage produced less information*, and produced it in the shape of
an alarm: three reds in the one subsystem a wipe stresses hardest, thirty seconds from being reported
as a divergence. **A false alarm is harder to catch than a false green, because alarm feels like
diligence.**

The repair is the one already noted below: absence should have ONE meaning. Fail when the fixture is
missing in CI, skip on an explicit opt-out — never a verdict that flips sign depending on the flag.

**A LOUD SKIP STILL COUNTS AS PASSED, and `cargo test` captures the announcement.** Measured on the
sibling suite the same evening:

    without the artifact   ...SKIPPED: set MISAKA_PALW_ARTIFACT…   9 passed   0.01s
    with the artifact      (no SKIPPED line)                       9 passed   9.70s

The skip prints — it was built to print — and **only `--nocapture --test-threads=1` surfaces it**,
which nobody runs by default. So "9 passed" included a test that checked nothing about the real
weights, and the captured log could not have said otherwise. *The duration is the evidence; the
count cannot be.*

**The design fix, for after the freeze: absence should be a DECISION, not a default.** An
artifact-dependent test should **fail** when its file is missing in CI and skip only on an explicit
opt-out. A skip that is the default state is indistinguishable from a check nobody wanted, and it
passes.

**The 18.40s is the evidence, and the dot count was not.** These fixtures decode a 1.79 GB artifact;
a skipped test is milliseconds. A green suite whose artifact-dependent cases quietly skipped would
print the same ten dots and the same `ok`. *So the instrument for "did the expensive check actually
run" is the wall clock, not the pass count* — and it is worth reaching for whenever a suite depends
on a file that may not be there. (5b measured 16.8s independently on their machine.)

What it now does, from the reproduction that produced `verdict: consistent` this morning:

    unbound   consistent-given-the-supplied-answer — binding_checked: false;
              NOT a statement that this artifact came from that inference
    bound     binding_checked: true, verdict: consistent — dsl_hash, artifact_hash and
              artifact_bytes recomputed over the bytes this claim's ids RENDER to under the
              tokenizer it pins, and output_root over those same ids
    refused   a DSL that is not the rendering of its ids; a tokenizer the claim does not pin
              (naming both hashes, never called a forgery); a missing artifact file

**Still open, and the announcement waits on it:** `binding_checked: true` is unreachable from the
shipped gateway, because nothing emits the `PalwJobContextV2` — only its hash. **The check exists
and the operator path cannot reach it**, which is the live-site-versus-search-order defect landing on
the fix for that defect. FH2 carries the context through the worker manifest and the gateway
response. *A disclosure saying "we check this" while no operator path reaches the check is worse than
silence — it is the same false assurance, in the security section.*

### The isolated boot has teeth — rehearsed on ibm, and it refused

Run against the `5f + impl` release binary on a throwaway appdir, unused ports, `--nodnsseed`, no
peers, live producers untouched throughout:

    [INFO ]  PALW court certified end-to-end for: PALW-BASE-0, PALW-QWEN36, PALW-QWEN25-A16
             (court_e2e_root 581466da…)
    [INFO ]  Consensus params fingerprint: 68e1e117…10516612 (network testnet-11)
    [ERROR]  panicked at consensus/src/consensus/utxo_set_override.rs:60:9
             genesis utxo_commitment mismatch (audit M-07)
               left:  ba2612417e7e0817…   what the premine BUILDS
               right: 2d882275dae82945…   what is PINNED
    Exiting...

**The node refuses to start rather than starting wrong**, which is the whole point of the step and
the first time this cut has seen it fire. That refusal is re-pin #1 not yet done — correct, because
the tree is not frozen — so this is the rehearsal succeeding, not the build failing.

**Three things it confirmed that had been reasoned about separately, each now from a running
binary rather than from a test:**

| | |
|---|---|
| the new commitment | `ba2612417e7e0817…` — the same value `print_premine_commitment` prints and `every_genesis_commits_to_the_premine_this_build_mints` reports as "builds". Two routes, one number. |
| the e2e root has NOT moved | boot prints `581466da…`, the pinned value. Confirms re-pin #2 is not live on this tree — the `8f08d303…` move belongs to an unmerged per-kernel drill rule. |
| the fingerprint | `68e1e117…10516612`, matching the pre-re-pin measurement taken independently. |

**So the ordering in §4b — build, then isolated boot, THEN stop and wipe — is load-bearing rather
than tidy.** A node that will not boot is survivable while the old chain is still up and is a dead
network fifteen minutes after the wipe. **The isolated boot must be green before any host is
stopped**, and this run is the evidence it can fail for a reason that is invisible to `cargo test`
until the re-pin is done.

## 5. Known-open, shipping anyway, stated so no page claims otherwise

### The split close is OPEN, and this is what it costs

Superseding this card's earlier paragraph, which described the release branch before the ADR-0082
merge and was marked stale rather than deleted so the two states stayed distinguishable.

| | |
|---|---|
| `max_close_chunks` | **27** on the RC preset, **1** on devnet; a wider declaration is refused at acceptance |
| when a declaration is legal | **only at Terminal** — the row cannot be opened at round 0 and held under an unfinished ladder |
| assembly deposit | `count × relay fee for 100,000 B` = **33,750,000 sompi (0.3375 MSK)** at 27 chunks |
| when the deposit is collected | on **every ending that is not the close it pinned**, both sides, all five endings — and on no ending that is |
| what it is charged against | **posted collateral, not an escrow.** A poor bond may declare and is charged what it has |
| when a group is swept | its own `assembly_deadline_daa` = declared + 4 × count, at most **108 DAA**, through the same conviction the backstop uses, before the backstop |
| declaration fee | `palw_certification_rent` is `None` on every preset, so the 28,125,000-sompi fee is **not charged**; a declarer pays 28 carrier fees plus the deposit only if it lapses |
| chunk journalling | O(1) deltas — 26 arrivals are **2,611,201 B** against 67,709,897 B in the whole-group form |

**The critical that made "open" dangerous is closed.** The per-block adjudication slot
(`PALW_COURT_CLOSE_MAX_PER_BLOCK = 1`) is now spent only by a move acceptance that was **admitted** —
the counter increments inside the `Ok` arm, after the fence, the signature and the folded state. So
one minimum-fee transaction per block carrying a forged `CourtAttnRootClaimed` can no longer deny
every chunk completion network-wide. That mattered more than a denial usually does: **a close denied
through its assembly window is not a delay but a conviction of the declarer.**

**The hybrid row is still not registered, and the reason has changed.** It is no longer "the split
path cannot be filed" — it can. It is that the hybrid's close is three carriers at every context
width, because it binds a recurrence rather than attention, and there is no width at which that
stops.

*One risk the fixer flagged and I am accepting: a good-faith declarer on the OTHER side is charged
when a verdict ends the session before its own assembly window closes. That is the right call — the
row held the room and will never deliver — and the alternative is one `if` if it proves wrong in
practice.*

**Faucet stays 0.5 tMSK.** The docs carry the real numbers instead: floor 11.2 MSK, A16 2,290,
QWEN36 3,868, plus an 8,333,316-sompi change floor. The faucet does not fund a bond and the pages
say so rather than implying it might.

---

### The ADR names a replay source the shipping code does not have

ADR-0082 Decision 7 says an opening asked for later is *"re-derived by replay from the checkpoint
chunks (`fp_interval`)"*. The fold retains **zero chunk bytes per position** — correct, measured, and
the thing that makes the capture affordable — so an executor serving a folded interval **replays
from the PROMPT**: one forward pass per opening served, not from retained chunks.

The verdicts do not change. The executor's cost per opening does, and by the ADR's own argument that
ratio is the practical lane's first number, so this is not cosmetic.

Closing it needs a retention shape — a live cache held across a claim's life — at one seam, and that
is not in this cut. **So the ADR's sentence is corrected rather than the code.** A document that
describes a stronger system than the one shipping mis-budgets the reader's suspicion: they stop
looking where the doc says the work is done. Fourth instance today, after the court's stale gap
list, the seed reader's format comment, and the free-prompt `tokenizer_id` cross-check that does not
exist.

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
| doc citations | `scripts/check-doc-citations.sh docs/…-genesis-card.md <merged-tree> palw-testnet-5f`, then the same without the second tree | **the two-tree form first**: it lists only citations that resolve differently on the two trees — the only ones a tree label can change the meaning of. Then the one-tree form against the merged tree, and READ the lines. A REPORT, not a verdict: it exits non-zero only on a missing file or line, because a citation that resolves to the WRONG line still resolves. |
| stranger | `scripts/misaka-palw-derive-stranger.py selftest` | recomputes the bytes in Python, independently of the Rust. **RED right now, and correctly so — read its two halves separately** (below). |
| third party | `scripts/misaka-palw-artifact-thirdparty.py --require` | mido / pygltflib / numpy-stl; compares MEANING (enclosed volume, playback duration) against the DSL |
| model gate, dense | `palw-model-gate` | A16 lane only — declared in advance |
| model gate, QWEN36 | `palw-qwen36-model-gate` | needs the ChatML fix (§7) to pass through the production assembly |
| fused-attention guard | `verify_class_admission_v5` | Refuses an `AttnFused` profile unless `palw_kary_court_active_at` — `FusedAttentionNeedsTheKaryCourt`, *"the class carries a fused attention site and this ruleset's court has no dissection to try it with"*; and `PricedForADifferentCourt { priced, court }` when the registered cost shape's arity is not the court's. **A guard on the way in, beside the drill and not instead of it.** |
| the split close is open | `check_court_close_declaration_acceptance_v2` (W6's signature), `check_close_declared_chunk_count_v2` (the ruleset's own `max_close_chunks`, so devnet's 1 refuses the path rather than engaging it), `palw_court_close_min_fee_v1` (an under-rented declaration is dropped with the block standing) | **Named so a reader can falsify §5 in ten seconds** rather than trust it. Every number here carries where it came from; a behaviour should too. |
| **prosecutability** | ADR-0082 stream I's end-to-end court drill | **This is the gate, and admission is not.** A graph-v5 leaf disputed to the bottom under the ARMED fence set, through `apply_object`: honest acquitted, forged convicted. F's admission arm refusing an unfenced `AttnFused` profile by name is a guard on the way in — useful, and not the property. The property is that a dispute can be carried to a verdict, and only the drill asserts it. |

**The stranger's two SKIPPED cases are the refusal cases, and the gap is structural.** Its per-file
output includes:

    91-over-max-steps.json          SKIPPED: cad op 'extrude' is outside this verifier's subset
    92-over-max-artifact-bytes.json SKIPPED: cad op 'revolve' is outside this verifier's subset

So `MAX_STEPS` (4,000,000) and `MAX_ARTIFACT_BYTES` (1 MiB) are confirmed **only by the
implementation under test**. `MAX_DSL_BYTES` is fine — `90-over-max-dsl-bytes` refuses for the same
reason on both sides, because a byte count is checked before any op runs.

**And it cannot be closed with a cheaper fixture, which is worth knowing before someone tries.** The
two limits are reachable only through the two high-amplification ops, and the kind's own doc says
why: *"The boolean reaches none of them, because `BOOLEAN_LEAVES_MAX` is DERIVED from the artifact
ceiling rather than declared: it is the largest leaf count whose worst-case lattice still fits. So a
boolean the grammar admits is never refused by a bound."* A `box`/`boolean` fixture — the ops the
verifier does implement — therefore **cannot** trip either ceiling by construction, and 64 KiB of
DSL cannot describe enough boxes to reach 1 MiB of STL anyway; it would trip `MAX_DSL_BYTES` first
and test the wrong limit.

Closing it means implementing ear clipping and revolve in the Python verifier — which is precisely
the `n³` work its refusal message declines. **Shipping with this open**, stated here rather than
discovered: the two ceilings an attacker would push on are pinned by one implementation. The skip is
loud, so it is a coverage gap and not a false green. *(That `BOOLEAN_LEAVES_MAX` is derived from the
ceiling rather than declared is the same one-spelling rule this cut enforces everywhere else — the
gap exists because the design is right, not because it is wrong.)*

**The stranger gate's red is a STALE PIN, and its summary line does not say so.** It prints
`SELFTEST FAILED — 3 of 18 checks disagree with the shipped tree`, which reads like a verifier that
disagrees with the build. It is not. The 18 checks are two different kinds and they must be read
apart:

- **Oracles 1–4 — all GREEN.** Every corpus file's `dsl_hash`+`artifact_hash` MATCH, every refusal
  refused for the same reason as the shipped tree, every corpus file has a golden entry, and a
  tampered `artifact_hash` is caught with exit 2. *This* is the property the gate exists for: an
  independent Python implementation reproduces the Rust byte for byte.
- **The 3 reds are all `recomputed X, pinned Y`** — `source_tree_sha256` and the two
  `transformer_id`s. Nothing is disagreeing with anything except a frozen constant, and it is stale
  for a reason the tree can name: two commits have touched `misaka-palw-derive/src/` since the pin
  was last set at `d16fb54e` — `a87cc282` (a formatting pass; formatting is inside the hash) and
  `81c1ca1d` (the alphaMode fix). This is re-pin #5, on schedule.

**So the failure mode to guard against is reading the summary line instead of the split.** A red
ORACLE would be disqualifying — it would mean the second implementation and the shipped one produce
different bytes, and the gate says so itself: *"a second implementation that is wrong proves nothing
and accuses the innocent."* A red PIN is a chore. Both print under one `SELFTEST FAILED`. Before the
cut this gate must be green outright; before then, check that the reds are exactly three and all of
them of the `recomputed/pinned` shape.

*(The pins live in `misaka-palw-derive/tests/`, outside the `src/` tree the hash covers, so writing
the new pin does not move the hash it pins. That is why §4 step 3 can be a single commit.)*

**On the MERGED tree** (`5f + adr0082-impl + the family branch`), measured by a session that wrote
none of the pins: `kaspa-consensus-core --lib` is **1,790 passed / 3 failed**, and the same three
with that session's own changes stashed — so nothing in the merge caused any of them. The three are
the premine commitment, the shipped fingerprints, and `palw_freeprompt_v3::tests::golden_vector_ids_are_frozen`.
The gate run there is **15 passed / 4 red, and all four are pins on the ordering above.** Nothing on
that tree is red for a reason the freeze will not clear.

**On this branch alone: 3,809 tests run, 3,807 passed, 2 failed** —
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

**The second red WAS the whole width story, and the move has since happened — this paragraph is kept
because its correction is the point.** It read: *"the ruleset's field is set to that same constant,
`COURT_MAX_STEP_LEAVES = PALW_STEP_MAX_LEAVES`, so W1b moved no width at all … the registered class
is capped at 39 positions."* That was true when written and is now false:

    consensus/core/src/palw_fp_devnet_v3.rs:367
    pub const COURT_MAX_STEP_LEAVES: u64 = PALW_RC_COURT_MAX_STEP_LEAF_COUNT;   // 2^26

The ladder moved to 2^26, `the_shipped_ruleset_admits_the_row_the_genesis_registers` is green, and
the 512 row clears it with 21.4% margin. **W1b's second half landed and this card went on saying it
had not**, for the ordinary reason: nobody re-reads a paragraph whose conclusion still feels right.
*A "known open" that has quietly closed is the same defect as a claim that has quietly broken — both
are the document disagreeing with the tree, and only one of them looks like good news.*

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
- **THE 512 ROW HAS NO GENESIS BUILDER, AND §2 SAYS IT IS REGISTERED.** Checked by looking rather
  than assumed: the class this entire card is about appears in **zero** `.rs` files on
  `palw-testnet-5f`, `palw-adr0082-impl` or `palw-artifact-names-genesis-row` — no constant, no
  `ClassRegistered`, no builder. §2's table lists it as registered at genesis and nothing registers
  it. The three that DO exist live in the profile modules, not `params.rs`, and that is where the
  fourth belongs:

      consensus/core/src/palw_qwen25_profile.rs:616   qwen25_a16_registration_v2   <- params.rs:4796
      consensus/core/src/palw_qwen36_profile.rs:1411  qwen36_registration_v3       <- params.rs:4789
      consensus/core/src/palw_base0_profile.rs:764    palw_rc_base0_registration_v1

  **It must carry a carriage**, because `verify_palw_genesis_v2` now refuses a fused row minted at
  genesis whose `ClassRegistered` has `admission: None` — `GenesisFusedRowCarriesNoProfile`,
  `GenesisCarriageIsNotTheClass`, `GenesisFusedDisagreesWithCatalog`. **Its signature is empty and
  that is correct, not a concession**: genesis verification never reads `.signature` (the only
  verifier is the acceptance layer at `consensus/src/pipeline/virtual_processor/processor.rs:6099`),
  a signature authenticates *who moved a permille from every incumbent* and at genesis nobody did,
  and it would anyway be checked against a `registrant_bond` key that the same genesis object list
  is creating.

  **FG MUST NOT ADD A SECOND A16 ROW AT WIDTH 512.** `bind --artifact` resolves without a
  `--model-id` because the width is unique in the table, and that uniqueness is a property of the
  TABLE, not of the code. The five A16 rows, printed by the tests rather than asserted:

      Qwen/Qwen2.5-1.5B                    16
      Qwen/Qwen2.5-Coder-1.5B-Instruct     18
      Qwen/Qwen2.5-1.5B/graph-v2           16
      Qwen/Qwen2.5-1.5B/graph-v3           16
      Qwen/Qwen2.5-1.5B/graph-v5@512      512   <- the only row at its width

  At 16 the route already refuses with `AMBIGUOUS at 16: [three rows]` rather than picking — which
  is the right behaviour and also the proof that a sixth row at 512 turns the working
  `bind --artifact` into an `AmbiguousAtWidth` refusal. It fails LOUDLY rather than binding wrong,
  so it is a liveness risk and not a safety one — but it would arrive on cut day as "certify
  suddenly refuses", against a tool that worked an hour earlier, at the moment nobody has spare
  attention. FG is the commit that would be adding rows near that width.

  **And the artifact→class pairing is now RUN rather than argued.** With the real 1,795,427,276-byte
  `qwen25-1.5b-a16.palwart` present, `the_shipped_artifact_names_the_row_genesis_registers` and
  `the_binary_binds_the_genesis_row_from_the_shipped_artifact` both pass (9.4 s and 9.6 s — they
  decoded the file). `decode_artifact_file_v1` recomputes the declared digest over every byte, so
  the id comes from **the file's own header**, not from a second computation of this build's. That
  is the pairing this section said nothing forced to agree.

  **The class id must be DERIVED and reported, never quoted into the builder.** `shape_profile_id`
  is `keyed64` over the borsh of the profile, so the id is whatever the profile derives to — and
  three adjacent ids are loose in this project's notes for three different things: `4277d84f…` from
  the registration/certification/artifact chain, `71bbb755…` which is what the panel actually
  registered at n_ctx 16, and `8d2e6f16…` which `palw-certify bind` produced from the artifact's own
  512 row. Reconcile the derived value against the artifact and the certification path before the
  freeze. *A class id quoted from a summary is what burned `n_ctx 17` on 2026-08-28.*
- **A one-line test for the 512 close, so the announcement can cite a command instead of a number.**
  `the_512_close_is_one_carrier`: assert `derive_court_cost_shaped_v1` over
  `palw_a16_context_row_profile_v5(512)` at `PALW_RC_COURT_MAX_STEP_LEAF_COUNT` is **one carrier**,
  and print the byte figure with `--nocapture`. Assert the CARRIER COUNT (stable, and the claim that
  matters); print the bytes (moved three times: 81,599 → 82,719 → 81,312). *This item exists because
  the announcement cited that test before it was written — the card-without-the-test defect, made by
  the person who had spent the day naming it, in the public-facing document.*
- **THE DERIVED-ARTIFACT BINDING — no shipped verifier ties an artifact to the inference it names.**
  Reproduced with the shipped binary: `palw-derive verify` recomputes `dsl_hash`/`artifact_hash`
  from the caller's ANSWER BYTES and `output_root` from the caller's TOKEN IDS, ANDs them, and
  prints `verdict: consistent` for an artifact whose DSL has nothing to do with those ids.
  `rendered_output_hash_v1` hashes the *ids* rather than the rendered text, so `output_root` carries
  no information about the answer bytes and the two arms cannot meet by accident. **An executor can
  attach any artifact of any kind to any of its own claims and every verifier in this release calls
  it consistent** — and the forger must be the claim's own executor, which is precisely the party
  ADR-0078 Decision 5 says a consumer should not have to trust.

  Every piece is already in the right crate, checked rather than assumed:

      misaka-palw-derive/Cargo.toml:30      misaka-palw-base0 is a real [dependencies] entry
      misaka-palw-base0/src/fp_worker.rs:851  render_answer_v1(&QwenTokenizer, &[u32]) -> Vec<u8>
      misaka-palw-derive/src/derive.rs:226    already calls misaka_palw_base0::* on this path

  so the missing step is one call — `canonicalize(render_answer_v1(tok, ids)).dsl_hash ==
  object.dsl_hash` — and the only input not in hand is the tokenizer, which `PalwJobContextV2`
  already pins by id.

  **Two parts; the first is mandatory even if the second slips.** (1) `verify` must never print an
  unqualified `consistent` for a binding it did not check — `binding_checked: false` and a word that
  does not read as "this artifact came from that inference". (2) `--artifact <path>` loads the
  tokenizer and performs the join, skipping by name when absent.

  **Ordering:** both files are under `misaka-palw-derive/src/`, so this moves all eight
  `transformer_id`s. It must land **before** the single `fmt --all` and before the tree is declared
  frozen, or the re-pin is stale on arrival. `scripts/check-derive-freeze.sh` is the check.

  *This is the one place the release's own promise — that a stranger can check a claim without
  trusting the claimant — is not kept, and it is the place the launch is named after.*
- **The 2^22 sweep (fixer FD2).** The ladder went to 2^26, but `2^22` survives as a bare literal at
  a set of sites the ladder change did not reach — and the 512 row's canonical job is **6,630,544
  leaves**, so until they move, *every honest claim of the class this genesis registers is refused
  by the transition*. `palw_freeprompt_v3.rs:1030` (`WorkLeavesAboveCap`) is the one that refuses on
  the acceptance path; the others are the E2E certificate's declared count, the genesis catalog
  entry's counts, the schedule's leaves, and `PALW_STEP_LEG_MAX_LEAVES` — if the leg shape pass
  bounds at 2^22 the row's leg cannot be committed at all. **Plus one the first sweep did not
  name:** `derive_court_cost_v1` (`palw_class_admission_v2.rs:293-295`) anchors `genesis_anchored_v1`
  at `PALW_STEP_MAX_LEAVES` = 2^22 while the RC ruleset ships 2^26, and three of its six production
  callers build the `court_cost` field of a `PalwClassCatalogEntryV2` — the registration row itself
  (`palw_qwen36_profile.rs:1422`, `:1502`; `palw_base0_profile.rs:805`). Either the catalog row and
  the v6 gate derive at different ladders and every genesis row is refused by its own cost field, or
  nothing compares them and the catalog publishes a cost for a court that will not play. The gate
  for this bullet is one end-to-end test: a 6,630,544-leaf v5 commitment passes every layer under
  the RC bundle, is refused **by name** under a 2^22 one, and the v5 row's catalog `court_cost`
  equals what the v6 gate derives under the RC bundle.

  *Why it was found this late: J's drill never reached stage 5, so no v5 claim had been executed
  through the chain. A cap nothing has yet exceeded is a cap nobody has yet seen refuse.*
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

  **And `--artifact` alone does NOT fix it — that prescription was wrong.** All four ids derived and
  printed on the merged tree, which is what turned a suspicion into a defect:

      a16 graph-v2/v3  n_ctx  16   7a76d29b…  fused false
      a16 graph-v5     n_ctx  16   1ae17978…  fused true
      a16 graph-v2/v3  n_ctx 512   8d2e6f16…  fused false   <- what `bind` produces
      a16 graph-v5     n_ctx 512   4277d84f…  fused true    <- what genesis registers

  `8d2e6f16…` is **graph-v2 at 512**. So `bind` is not merely binding the wrong WIDTH, which is what
  this section assumed — it binds the wrong GRAPH, at the right width, differing only by the fused
  site. Deriving from the artifact does not help by itself: the artifact header declares geometry
  (family, width, eps) and **no graph**, so `bind --artifact` projects it through
  `a16_row_for_artifact_shape_v1` → `palw_a16_context_row_profile_v1` and lands on v2 again.

  **The correct derivation already exists and is already tested.** J shipped
  `classes::a16_artifact_row_v1`, which projects the same header through
  `palw_fuse_attention_site_v5` over the graph-v3 dense row with the artifact's own epsilon and the
  tiled v3 map — **with an equality test pinning that it derives the same `shape_profile_id` as the
  genesis row.** The fix is that `bind` calls the other one. *The forcing test existed and the
  production path took the other spelling anyway*, which is this project's fourth instance of one
  class root spelled twice with nothing making them equal.

  **The test to add is not "the two derivations agree" — J has that and it did not help.** It is
  that `bind --artifact` over `instruct-bound.palwart`, through the binary's own argument parsing,
  prints `4277d84f…`. A unit test that builds both ends from one source of truth checks nothing.

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

  **The swap is verified, not assumed.** The bound file was put through `palw-certify bind
  --artifact` and it decodes — 1,795,427,276 bytes, declared digest recomputed over every byte —
  and **names the same class as the shipped one**, with the same twelve reachable kernels covered by
  the same family. So binding the tokenizer does not move the class the artifact names, which is
  exactly what the replacement needed in order to be safe.

  The bound artifact must replace the shipped one at the cut; `from_registered_profile` refuses the
  unbound file, so the dense SDK path does not work until it does.

  **Distributing it costs 128 bytes, not 1.8 GB.** Every host already holds the shipped artifact,
  and the bound one differs from it in exactly two 64-byte fields. Writing those two fields in place
  and checking the result's sha256 against the converted file's is a complete verification — the
  hash covers every byte, so a wrong patch cannot produce a right hash, and there is no partial
  state between "identical" and "not". Done and confirmed on the fleet's first host:

      sha256(bound artifact, converted here)   3f8fc5066bafae28d81b2360227a08e43fdb961ee6355938c56d32edf19d7623
      sha256(shipped artifact, patched there)  3f8fc5066bafae28d81b2360227a08e43fdb961ee6355938c56d32edf19d7623

      offset 1,777,209,032   fa9a4352…a649bb   (tokenizer commitment, was 64 zero bytes)
      offset 1,795,427,212   158314b5…bf6450   (container digest)

  The alternative — transferring 1.79 GB per host over a link measured at 2 MB/s — costs half an
  hour each and is not more trustworthy, because it would be verified by the same hash.
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
- [x] **`(v − max) << up` wraps i64 before the clamp** in `softmax_shifted` and `a16_attn_exp_one`
      for `up_bits ≥ 47` — a key 40,000 below the maximum receives full weight. **MEASURED on the
      shipped artifact: the maximum across all 28 layers is 25.**

          layers 28    values {14, 15, 16, 18, 21, 25}    max 25    threshold 47

      Read out of the artifact's own `attn_softmax_up` entries, one byte per layer, on the bound
      file. **So the fix moves no committed value for the class being registered** and is free
      whenever it lands, rather than free only before the cut. It still belongs in the arming set —
      a class at a higher `up_bits` would be silently wrong — but it stops being an ordering
      constraint on the freeze.
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
