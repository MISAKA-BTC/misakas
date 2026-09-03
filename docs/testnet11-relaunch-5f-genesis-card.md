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
| `palw_context_ladder` | **`None` — DO NOT ARM** | **This row has now been wrong twice and is right for a third reason.** It first said ARM on a false premise. It then said DO NOT ARM because *"nothing reads the fence"* — true on `palw-testnet-5f`, where `palw_context_ladder_at` does not exist and the processor never reads it, and **false on `palw-adr0082-impl`**, where the ClassRegistered arm (`processor.rs:6162`) and 5b's admission-shape accessor both read it. *Worse, I argued it from the absence of `palw_context_ladder_active_at` — a name I invented; the real one is `palw_context_ladder_at`, so my sentence stays true while its conclusion went false.* **The reason that survives is the shape, measured on the preset that ships:** t11 is `court: Some(arity 2, Flat ids, window 3000), ladder: None`, and that is the shape FG proved the 512 row admits under. Arming would move t11 to devnet's shape — **the one nothing has validated for t11's registered set** — and move the fingerprint for a rule only post-genesis registrations would feel. *Do not arm it because the shape it would produce is the unmeasured one.* |
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

**ALL THREE PROPERTIES THIS NIGHT MANUFACTURES ARE NOW MEASURED, AND ALL THREE ARE GREEN.**

    a node killed partway through recovery comes back safe   impl 41e364b6   242s, 7 restarts
    a stronger chain found during IBD wins                   d7957910       609s
    bootstrap recovery crosses a provisional pruning point   d7957910       620s

**The third is the scar.** A joining node crossing a provisional pruning point is exactly where this
project put a node into permanent quarantine before. It passes.

*The two on `d7957910` are one merge behind impl; IBD participation is not a surface FH, FH2 or the
fmt touches. Caveat stated, not discovered.*

**AND THE IGNORE STRING ITSELF CARRIED A CLAIM NOBODY WAS CHECKING.** Those two tests are marked:

    #[ignore = "passes; opt-in because it takes ~6 minutes — run with --include-ignored"]

They took **10.2 minutes each** — the stated runtime is 70% low. That is trivial. **The word
`passes` is not.** It is an assertion about the *result*, living on the one test that never runs to
produce a result — and it is read at exactly the moment someone is deciding not to run the thing
that would check it. Had these started failing, the attribute would still have said `passes`, and
the next person weighing the cost would have read a reassurance.

**An `#[ignore]` reason may say WHY. It must not say what the result would be.** That is the
favourable-staleness shape one level further in than a document, and worse placed: a stale doc is
read by someone reading; a stale ignore string is read by someone deciding *not* to look.

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

### A SHORTCUT THAT HAPPENS TO BE RIGHT IS WORSE THAN ONE THAT IS WRONG

`cargo run -p misaka-palw-derive --bin palw-derive -- drill --report r.json` builds **that one
binary**. The drill then shells out to `palw-evm-runner`, does not find it, and six of forty-six
goldens refuse — ADR-0079 SA-1's confinement gate holding, exactly as the card already documents for
the `--lib` case. **The report's own `verdict` field said `FAILS`.** Its `transformers[]` array sat
right beside it, populated and inviting.

Built the crate properly (`cargo build -p misaka-palw-derive`), re-ran, got a valid report — *46
goldens, 0 mismatched, 8 bounds enforced* — and compared:

    8 of 8 transformer ids IDENTICAL between the FAILS run and the valid one
    source_tree_sha256 identical: 637858dba5ea5e34b9459a580b2b81d1361aecf450bc615a4ee9621d4953a988

**The ids do not depend on the EVM runner. So pasting them out of the failed report would have
produced exactly the right genesis — and nothing, ever, would have told anyone the process was
wrong.** The values would have been correct, the pins would have been green, the chain would have
launched, and the next person to follow the same shortcut would have had no reason to doubt it —
until a change *did* make the ids depend on what the failed leg covers.

*A wrong answer gets corrected. A right answer from an invalid process gets INHERITED.* That is why
the rule is procedural and not a matter of judgement: **check the report is valid before reading any
field of it.**

**And the first version of that rule said "assert the report's `verdict`" — which does not work,
because the report file has no `verdict` field.** The verdict is printed on **stdout only**. Both
files carry an identical key set:

    arch bounds golden os refused rows schema source_tree_sha256 transformers uncovered

**A `--report` file, read on its own, is structurally identical for a FAILS run and a valid one.**
The discriminator exists but it is one level in:

    report["golden"]["mismatched"] == []     valid: []   failed: six entries
    report["uncovered"] == []                [] in BOTH — this one does not discriminate

So the executable rule is: **`golden.mismatched` must be empty**, not "check the verdict". *I wrote
the rule from what stdout told me and it did not survive contact with the artefact it was about* —
which is the same defect one turn later, in the correction for it.

*The deeper reading: a report designed to be machine-read carries the ids and not the judgement of
whether the run that produced them was valid. That is a defect in the report's shape, worth an
issue after the cut — the file a tool writes for another tool should not be the one that omits
whether it worked.*

### SEND THE VALUE **AND** WHAT YOU THINK PRODUCED IT — the disagreement is the check

A hash arrived with a sentence: *"derive/src tree hash after the fmt: `4969f8dc…` (the fmt moved it
as expected)."* **The hash was right. The sentence was wrong** — that is the value from *before* the
fmt too, and `git diff` across the fmt shows nothing under that crate. The fmt ran (17 files, 211
insertions) and touched base0, the tests and the sdk; `misaka-palw-derive/src` was already formatted.

The cause was two hashes over one directory that are easy to conflate: **`source_tree_sha256`**, the
project's own hash and the transformer ids' input — which **FH did move**, `d2419027…` →
`637858db…` — and the **git tree hash** of the same path, which moved at neither FH nor the fmt in
the window being discussed. Same content, different algorithms, and an intuition that applies to one
reads as applying to the other.

**A bare hash would have passed unchallenged.** The only reason it did not is that the sender said
what they believed had produced it, and the belief and the value disagreed. *So the ceremony's
reporting rule is: never send a value alone — send the value and the operation you think produced
it.* It costs a clause and it converts every handoff into a two-ways check.

**And the correction was in our favour**, which is the part that would otherwise have gone unnoticed:
if the fmt moved nothing under that crate, the prediction captured *before* it is the exact tree the
pin comes from, and the "two readings minutes apart" hazard this ceremony was built to guard has
nothing to guard.

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

Measured: the three worker-dependent agent tests "failed" in **0.016s** each under `--run-ignored
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

---

## The instrumentation I wrote to measure the last open gap had two defects, both already written down

The three worker-dependent agent tests are the one measurement still open. I drove them
on ibm from a script — `/root/agent-recovery.sh` — and the script contained **two defects
this session already has notes about**. Neither produced an error.

### 1. `| tail -4` on the build, again

```
nice -n 19 cargo build --release … --bin palw-worker 2>&1 | tail -4
```

The four lines that survived were: half of a doc comment, `note: run with RUST_BACKTRACE=1`,
`warning: build failed, waiting for other jobs to finish...`, and `WORKER MISSING`.
**The `error:` line is above the window.** The log is 273 bytes and says the build failed
without saying why — so `grep -n '^error'` over the whole file returns nothing, and the
file reads as though there were no error at all.

This is the second time today. The rule is not "use a bigger tail":

> **Truncate the summary, never the diagnostic.** A tail is a display decision applied to
> a stream whose important line is at an unknown position. Write the whole thing to a file
> and grep it; `tail` the grep, not the build.

### 2. `pgrep -f agent-recovery.sh` matched the ssh command that was asking

My first check printed `script: 2` and I read it as "the run is in progress". The script
had **exited 1 twenty minutes earlier**. Both matches were shell processes whose *command
text* contained the string — one of them the `ssh … pgrep …` I had just typed.

I have a memory file about exactly this (`pgrep -f の待機ループは自分に一致する`) and it
did not fire, because last time the shape was a `until ! pgrep -f` **wait loop** and this
time it was a one-shot **census**. The stored lesson was attached to the loop, not to the
predicate.

The second invocation printed the proof in its own output:

```
2505482 bash -c … pgrep -af "bash /root/agent-recovery.sh" …   <- the asker
2505485 bash /root/agent-recovery.sh                            <- the actual script
```

> **A process census that includes the censor is off by one, and the one is always yours.**
> Match on the executable (`pgrep -x`), or on a pid you recorded when you started it, or
> require the count to drop rather than to be nonzero.

### What this cost, and what it did not

Nothing in the cut moved: the four re-pin values are unaffected, and the tests concerned
are already listed under §5 as a **stated gap**, not as a green. What it cost was twenty
minutes of believing a measurement was running when it had failed, which is the cheaper
half of the same defect — the expensive half is believing a measurement *passed*.

The rerun captures the full build output and is running now. **Whatever it says, the honest
line in §5 is unchanged until the tests are observed, and if the build error turns out to
be something other than the pinned-llama.cpp link I named, then my stated reason for the
gap was wrong even though the gap was real** — a true fact with a fabricated cause, which
is the form that survives review because the conclusion checks out.

### Correction, same hour: I named the group after one of its three files

I have been calling them *"the three `palw_agent_recovery` tests"*, and 1c wrote the phrase
back to me in their ledger. They are **three tests in three different files**, one each:

```
palw_agent_recovery::a_dead_agent_costs_a_delay_and_not_a_tag
palw_agent_concurrency::concurrent_seeds_are_faster_and_are_the_same_tags
palw_agent_equivalence::the_resident_agent_and_a_fresh_process_compute_the_same_tag
```

The number was right; the **address** was wrong. And the address is what a command consumes:
my script ran `cargo test --test palw_agent_recovery -- --ignored`, which builds and selects
**exactly one binary**. It would have printed `1 passed` and I would have reported that the
three had run. *A group named after the member you happen to know will be selected by that
member's name.*

1c got the complete set — but by tooling, not by knowing the layout, having never checked
the files either:

```
cargo nextest run --run-ignored all --no-fail-fast \
  -E 'test(the_resident_agent_and_a_fresh_process_compute_the_same_tag) or
      test(concurrent_seeds_are_faster_and_are_the_same_tags) or
      test(a_dead_agent_costs_a_delay_and_not_a_tag)'
```

> **Prefer the selector whose output proves its own reach.** nextest names the *binary* on
> every result line, so three distinct binaries in the output is the evidence the filter
> found all three. `--test <file>` prints `1 passed` and looks identical to a complete run
> of a one-member set. **The count in the result is not evidence about the size of the
> intended group unless the result also names what it selected.**

### And the gap's stated cause was right while its consequence was not

The build panic is verbatim what §5 claims:

```
misaka-palw-worker links the PINNED llama.cpp build, which this repository does not
contain, and MISAKA_LLAMA_SRC is not set.
```

But **`misaka-palw-pow-driver` does not depend on that crate.** The tests spawn the worker
as a **subprocess** from `$PALW_WORKER`. They need a *binary*, not a *build* — and ibm has
two prebuilt ones (Aug 14 and Aug 16), both of which execute. So the sentence *"they cannot
run"* did not follow from the true fact I attached it to.

> **A correct cause with an unchecked implication is harder to catch than a wrong fact**,
> because the quotable part survives every review. The thing to check is not the citation;
> it is the *therefore*.

Same shape, one line further: I have been carrying **"`palw-worker` runs on all three fleet
hosts."** `pgrep -x palw-worker` on ibm is **0**. The binaries are present and executable
and nothing is running one — the re-executor spawns it on demand. *"Runs on" carried an
implication nobody tested.*

All three are running on ibm now against the real 1.2 GB pinned model. **Two caveats stated
before the result rather than after it:** the ibm checkout is `8923b354` detached — **not
the tree being cut** — and the worker binary is three weeks old, so an equivalence pass is a
pass against a stale counterpart. Whatever colour it comes back, it is evidence about the
property and not about 5f's copy of these tests, and §5 keeps its stated gap.

### The run happened, it went red, and the red is about the fixture — measured, not inferred

`a_dead_agent_costs_a_delay_and_not_a_tag`, on ibm, with the real 1.2 GB pinned model:

```
test result: FAILED. 0 passed; 1 failed   finished in 41.36s      (WALL 131.26 s)
panicked at palw_agent_recovery.rs:79:5:
  no resident agent was running to kill — the first seed did not use one
```

**41.36 s is the first thing to read.** The `--run-ignored all` version of this failure took
**0.016 s** and was a missing env var. This one loaded a model and ran. *The wall clock is the
evidence a test ran* — and here it is what separates a real red from a fixture-shaped one.

The panic comes from `pkill -f "palw-worker --mode pow-agent"` exiting non-zero, which the
assertion renders as *"the first seed did not use one"* — a sentence about the **driver's
choice**. The actual cause is one level down and is not a choice:

```
/root/palw-drill/bin/palw-worker   built 2026-08-16   strings | grep -c pow-agent = 0
/root/palw-class/palw-worker       built 2026-08-14   strings | grep -c pow-agent = 0
```

**The binaries predate the mode.** `--mode pow-agent` is not an option they can refuse; it is a
string they do not contain. The driver spawned one, the worker printed its usage line and exited,
the driver fell back to one-shot — which the test's own comment calls correct — and it did so at
seed one, where the test needs the agent alive to kill it.

> **`pkill` exiting 1 means "matched nothing", which is not the same as "nothing was started".**
> A limit rendered as a verdict, once more, and this time the verdict names a *decision*
> ("did not use one") for what is really an *absence*. The message would have sent a reader to
> read the driver's fallback logic, which is correct code.

### The fleet fact underneath it, which is the part that outlives this test

Measured across all three hosts, every `palw-worker` on disk:

```
169.58.39.220    /root/palw-drill/bin/palw-worker   2026-08-16   pow-agent=0
                 /root/palw-class/palw-worker       2026-08-14   pow-agent=0
169.58.232.113   /root/palw-class/palw-worker       2026-08-26   pow-agent=0
5.104.81.23      /root/palw-class/palw-worker       2026-08-14   pow-agent=0
```

**Four binaries, three hosts, three build dates, zero of them have the resident-agent mode.**
Running processes: 3 on ibm — all children of the test run above — and **0** on the other two,
which are the hosts carrying kaspad.

So the claim I had been repeating, *"`palw-worker` runs on all three fleet hosts"*, is wrong twice
over: it is not running on two of the three, and none of the deployed binaries can do the thing
the phrase implies. **Nothing in the announcement or the runbook makes a claim about the resident
agent** — checked, no hits — so there is no public sentence to retract. This one lived only in
my own prose, which is where it would have leaked from.

### What §5's line should say instead

Not *"the three tests cannot run because the crate links a pinned llama.cpp."* That is a true
sentence attached to the wrong noun. The accurate version:

> These three tests need a **worker binary built from a tree that has `--mode pow-agent`**.
> Building one requires an external llama.cpp checkout at `LLAMA_COMMIT`, **built, not merely
> cloned**, which this repository does not contain and the fleet has never had. Every deployed
> worker predates the mode. `cargo build --release` skips the crate by default, so **the relaunch
> will not produce one either** — the fleet's behaviour here is the same before and after the cut.
>
> By the crate's own build script: *"No node needs it to produce or verify a block: since ADR-0053
> there is one execution family and it is BASE-0's, which is pure Rust in this tree."*
> **These tests exercise a path that is not on the block-production path.**

That last sentence is the one that decides how much the gap costs, and it is the one my original
phrasing never reached — because I stopped at a true fact about llama.cpp and never asked what
the subsystem was *for*.

### The second red carries a green inside it, and the verdict throws the green away

`concurrent_seeds_are_faster_and_are_the_same_tags`, same host, same stale worker:

```
permits: 3 (pool warmed in 156.099172681s)
3 seeds serially through the pool: 52.802461904s
3 seeds concurrently:              113.394920626s
speedup: 0.47x
FAILED   ... finished in 358.11s   (WALL 375.15 s)
  concurrent seeds were not faster than serial ones — the permits are not being used
```

**The assertion that failed is the last one in the test.** Everything above it ran and passed:

```
assert_eq!(inference_concurrency(), PERMITS)          the permit gate took the count
expected[i] = one_shot_tag(worker, seed_i)            three fresh-process baselines
assert_ne!(expected[0], expected[1])                  anti-vacuity: the seeds differ
assert_eq!(tags[i], expected[i])   x3                 <- THE CONSENSUS PROPERTY, GREEN
```

So this run established, on real hardware with the real pinned model:

> **Three concurrently-computed Layer-1 PoW tags each equal the tag an independent fresh
> process computes for the same seed**, with the test's own anti-vacuity guard confirming the
> three seeds are genuinely distinct.

That is the property consensus depends on. **It is green, it was measured here for the first
time, and `test result: FAILED` is what the log records.** What actually failed is a *speed
direction* — and the speed direction is the one property a stale counterpart is guaranteed to
invert: with no `pow-agent` mode in the binary, "concurrent" means three simultaneous 1.2 GB
model loads on a 23 GiB box, so 0.47x is the arithmetic of the missing feature, not a defect.
The message *"the permits are not being used"* is literally true and names a mechanism where
the cause is again an absent binary feature — **the same mis-attribution as the first test's
message, in a second test, written by the same hand.**

> **A verdict is a reduction, and a reduction over `assert` is `AND`.** One failed clause turns
> a run with four established facts into the word FAILED. **This is the exact inverse of the
> `--report` defect from an hour ago**: there, the file carried the data and omitted whether the
> run was valid; here, the run carries the validity and discards the data. Both are the same
> break — *the join between a measurement and its standing is lost* — and they fail in opposite
> directions, so no single habit catches both. **Read the assertions that passed before the one
> that failed.** They are not consolation; on this run they are the only new evidence.

The order matters and is not luck: the author put the cheap correctness assertions first and the
host-dependent timing assertion last, with a comment saying the magnitude is *"a property of the
host's cores, not of this code"*. **The test was built so that a failure of the weak claim comes
after the strong ones have been established** — that is a design worth copying, and the log
format is what hides it.

### The third one PASSED, and its pass is the finding — the anti-vacuity guard is weaker than its own comment

```
the_resident_agent_and_a_fresh_process_compute_the_same_tag ... ok
one-shot, 3 fresh processes:                        45.749246189s
resident agent, 1 process including the model load: 44.593963373s
test result: ok. 1 passed   finished in 90.34s
```

**Green. And the worker binary contains the string `pow-agent` zero times.** There was no
resident agent. The driver fell back to one-shot, so the test compared three one-shot tags
against three one-shot tags and found them equal — which they are, trivially.

The author **knew this failure mode and wrote it into the file**, immediately above the guard:

```rust
// And the agent was genuinely used. Every failure inside it falls back to the one-shot path,
// which is the right behaviour and would also make this test pass while proving nothing — so
// the cost is the evidence: a silent fallback runs the same three processes and lands within
// noise of the baseline, NOT SEVERAL TIMES UNDER IT.
assert!(agent_elapsed < one_shot_elapsed, "... it most likely fell back, making the equality above vacuous");
```

The prose names the right discriminator — *several times under*. The code encodes `<`.

```
one-shot   45.75 s      agent path 44.59 s
margin     1.16 s = 2.5%          ratio 1.026x
guard as WRITTEN    agent < one_shot          -> PASSED
guard as COMMENTED  several times under       -> would need ~15.2 s
```

**A silent fallback makes the two numbers two samples of the same distribution, so `agent <
one_shot` is a coin flip. The guard catches the vacuity it was written to catch about half the
time, and this run is the other half.** The prose is not decoration here — it is the correct
specification, sitting one line above an implementation that does not meet it.

> This is [[boundary-tests-pin-the-off-by-one]] in its purest form yet: **the comment states the
> property and the assertion pins something weaker, so the test's own author left the evidence
> that the test is wrong inside the test.** And it is *the same defect as a doc comment cited as
> if it were code* — prose that is right next to code that is not, where the prose is what gets
> read and the code is what runs.

**Fix, post-cut, one line:** `assert!(agent_elapsed * 2 < one_shot_elapsed, …)` — the comment
already justifies the constant, and a genuine resident agent pays one model load instead of
three, so 2x is conservative against the 3x the design implies.

### The three, together — and the shape they make

```
palw_agent_recovery       FAILED  41.4s   honest red: fixture; no information about the property
palw_agent_concurrency    FAILED 358.1s   red on SPEED; contains a real green (3 concurrent
                                          tags == 3 fresh-process tags, non-vacuity guarded)
palw_agent_equivalence    ok      90.3s   VACUOUS: compared one-shot to one-shot; its own
                                          guard would have said so with a ratio instead of `<`
```

**The one that passed is the one that proved nothing, and the two that failed are where the
evidence is.** Nothing about the colour column is a guide to what the run established.

*What this run actually establishes*, and all it establishes: **three concurrently-computed
Layer-1 PoW tags each equal an independent fresh process's tag for the same seed** — from the
concurrency test's pre-speed assertions, on real hardware, with the real 1.2 GB pinned model and
an anti-vacuity guard that did hold. That is worth having. It is not "the three agent tests
pass", and §5 keeps its stated gap.

*Caveats, as promised before the run:* the ibm checkout is `8923b354` detached, **not the tree
being cut**, and every worker involved is three weeks stale. Neither of those weakens the tag
equality — it is an equality between two paths measured in the same run — and both of them are
why nothing here is offered as a green for the cut.

### Third time in one hour: a true sentence whose implication I did not check

Twenty minutes ago I wrote that building a worker with `--mode pow-agent` *"requires an external
llama.cpp checkout at `LLAMA_COMMIT`, **built, not merely cloned**, which this repository does
not contain and the fleet has never had."* Every clause of that is true. Then I stopped.

**This machine has one, built:**

```
/Users/wata/Downloads/misaka-palw-runtime/llama.cpp-cpu/build/CMakeCache.txt      Aug 11
/Users/wata/Downloads/misaka-palw-runtime/llama.cpp-cpu/build/src/libllama.a
                                          .../build/ggml/src/libggml.a
                                          .../build/ggml/src/libggml-base.a
                                          .../build/ggml/src/libggml-cpu.a
```

CMakeCache plus all four static libraries — **exactly the shape `build.rs` demands**, and the
`-cpu` suffix matches the `MISAKA_PALW_CPU=1` profile the same file describes. The sentence I
wrote scoped its claim to *the repository* and *the fleet*, and the reader's conclusion — and
mine — was **"therefore nobody can build it"**. Nobody said that. Nobody checked the third place.

> **Three times this hour, in three unrelated subsystems: the fact was right and the *therefore*
> was never tested.** `llama.cpp is absent from the repo` → *therefore the tests cannot run*
> (they need a binary, not a build). `palw-worker exists on all three hosts` → *therefore it
> runs there* (`pgrep -x` says 0). `the repo and the fleet lack a built llama.cpp` → *therefore
> it cannot be built* (the build machine has one). **A citation checker catches none of these,
> because in all three the citation is correct. What needs checking is the arrow.**

Recorded, not acted on: closing that gap means building `misaka-palw-worker` against this
checkout and re-running the three tests here. It is off the block-production path by the crate's
own statement, so **it is not a precondition for the cut** — but §5 must now say *"unbuilt here,
buildable on the release machine"* rather than anything that sounds like *"unbuildable"*.

## The free-prompt gateway, driven end to end — and the finding I nearly filed against this card

The goal names *自由プロンプト* beside MIDI and 3D, and the operator-facing path had never been
run. It runs. On **5f**, with the real A16 artifact and tokenizer:

```
[1] health ok — template misaka-palw/fp-gateway-template/chat-segments/v1, n_ctx 16,
    chain anchor file …/anchor.json
    registered / fp_certified / bond_active / exposure_room  -> all four present, all "unknown"
    can_submit -> false
```

**ADR-0077 Decision 3 holds against a live gateway**: all four chain names present in `/health`,
every one `unknown` in the offline form, and a gateway with no `--rpc` says it cannot submit.
Then the worker refused the job on width:

```
the worker refused the job: prompt 36 + decode ceiling 24 exceeds max_context_tokens 16
```

Which is §2's own argument arriving from the operator's end: **the certification path, the manifest
handshake, the class check and the width check all work, and width is the only thing left.**

The QWEN36 lane runs too, and refuses on its own class boundary first — correctly:

```
qwen35-2b.palwq36 is not a Qwen3.6-35B-A3B/graph-v3 artifact: the layer stack is not this
class's (24 layers)                                    <- the class check doing its job
class_id mismatch — ours 2e91c9d3…, request c1c1c1c1…  <- the identity check doing its job
prompt 36 + decode ceiling 24 exceeds max_context_tokens 8
```

### What I nearly reported, and what stopped it

The running 5f worker announces `71bbb755…` at n_ctx 16, from a hardcoded const:

```
palw-testnet-5f:  const MODEL_ID: &str = "Qwen/Qwen2.5-1.5B/graph-v2";
```

This card says, at the dense-lane section: *"the class the WORKER embodies is not held —
`palw-a16-fp-worker`'s `MODEL_ID` is the v5 512 row."* Measured against 5f, that reads false, and
I was one command from filing it as a defect **in this card**. Checking the other tree first:

```
palw-adr0082-impl:  const MODEL_ID: &str = misaka_palw_base0::classes::A16_GRAPH_V5_MODEL_ID;
```

**The card is right, about impl. I ran 5f.** The statement is accurate for the integration branch
and unlabelled as to tree — the same defect `check-doc-citations.sh` exists for, in the one form
that tool cannot catch, because there is no `file.rs:NNNN` in it to resolve.

> **The habit that saved it was checking both trees before asserting, not checking harder.**
> Every wrong thing I have said today would have survived a more careful reading of the single
> source I was looking at. *Ask the same thing two ways* is not a thoroughness setting.

So the true state, with the tree named for every clause: **on 5f** the FP worker serves graph-v2
at 16; **on impl** it points at the v5 512 row; **the merge is what moves it**; and §7's
*"THE 512 ROW HAS NO GENESIS BUILDER"* remains the binding item, unchanged by any of this.

### Two real defects on a shipped path, found by running the documented command verbatim

`misaka-palw-gateway` **is** in `default-members`, so this is a path the cut ships.

**1. The documented invocation cannot work as written.** `docs/palw-freeprompt-gateway.md`:

```bash
python3 scripts/misaka-palw-fp-gateway-smoke.py ./target/release/misaka-palw-gateway \
    ./target/release/palw-a16-fp-worker "$MISAKA_PALW_GGUF"
```

Run verbatim it fails three times in a row, each on something the doc never mentions:

```
cannot spawn ./target/release/palw-a16-fp-worker: No such file or directory
        (relative paths — the gateway does not resolve them from the caller's cwd)
fatal: MISAKA_PALW_NETWORK_ID is not set        <- checked FIRST, before the artifact
fatal: MISAKA_PALW_ARTIFACT is not set          <- and then MISAKA_PALW_TOKENIZER
```

The usage line advertises **three arguments** and the worker needs **three more environment
variables**, none of which appears anywhere in that document — `grep` for either name returns
nothing. *An interface that documents a third of what it requires.* And the ordering is worth
keeping: `NETWORK_ID` is refused first, with the best error message in the subsystem, which is
why I stopped predicting which variable would fail and read what it said.

**2. The smoke's synthetic identity cannot pass a class-checking worker.** It writes
`"class_id": "c1" * 64`, which every worker that checks its own id refuses. It works today only
against a worker that does not check. Running it against one that does needs the real id
patched in, which is what I did.

Neither blocks the cut. Both are the operator's first five minutes.

### A named pre-announcement gate: the free prompt must produce an ANSWER, not a correct refusal

The run above demonstrated the handshake, the class check and the width check. **It never
demonstrated an inference.** The width wall on 5f is structural, not configuration:

```
palw-testnet-5f       const MODEL_ID = "Qwen/Qwen2.5-1.5B/graph-v2"      -> n_ctx  16
palw-adr0082-impl     const MODEL_ID = classes::A16_GRAPH_V5_MODEL_ID    -> n_ctx 512
palw-adr0082-into-5f  const MODEL_ID = classes::A16_GRAPH_V5_MODEL_ID    -> n_ctx 512
```

There is no environment override on the A16 worker; the one lever is the *qwen36* worker's
`MISAKA_PALW_MODEL_ID`, and every qwen36 canonical row shares `QWEN36_RC_CANONICAL = (7, 2)`, so
all of them land at 8. **No configuration of 5f admits a 60-position job.** The merge is what
moves it, and after the merge the 5f worktree is already built, so the rerun costs a rebuild and
a script rather than a fourth cold worktree — `/private/tmp` is at 87% with 55 GiB free, and a
cold target is 7–62 GiB.

> **GATE — after the merge, before the announcement.** Rerun the FP gateway smoke on merged 5f
> with the 1.79 GB `.palwart` and the qwen2.5-1.5b tokenizer.
>
> **Green: the announcement may say a free prompt produced an answer.**
> **Red or unrun: it says the width check works.** Only one of those is earned by what is
> measured today, and it is the second.

Two predicted stoppers, written before the run so a red is reported rather than explained:

1. **The artifact must match the v5 row's declared shape.** 1c's
   `the_shipped_artifact_names_the_row_genesis_registers` passes in 9.90 s against the real
   1,795,427,276-byte file, so the header digest lands on `4277d84f…`. If the worker resolves its
   class the same way, that half is already evidence and need not be re-derived.
2. **`MISAKA_PALW_TOKENIZER`.** The v5 row pins an *inventory* root rather than a container
   digest, so a tokenizer binding must not move the class id. **If a tokenizer mismatch surfaces
   as a wrong class id rather than as its own named refusal, that is a finding about the
   inventory-root property — not a fixture problem** — and it gets reported as one.

### The rule this evening earned, applied to claims rather than to numbers

Every measurement in this card names its tree: `8923b354` detached, a three-week-old worker,
`d7957910` one behind. **The claims do not**, and that asymmetry is what nearly produced a false
finding against this card an hour ago — a number looks incomplete without provenance and a
sentence does not.

> **A statement about code names its tree in the same sentence, or it is defective for not
> naming it.** `check-doc-citations.sh` cannot enforce this: there is no `file.rs:NNNN` in
> *"the worker's MODEL_ID is the v5 512 row"*, so there is nothing for a citation checker to
> resolve. **A claim about which tree a fact holds in is not a citation, and we have no
> instrument for it** — only the habit.

### The structural wall, stated positively

The width limit is worth recording as a property rather than only as a blocker:

> **Nobody can configure their way past the served width.** The A16 row is a hardcoded const
> with no environment override, and the one lever that does exist — the qwen36 worker's
> `MISAKA_PALW_MODEL_ID` — lands every canonical row at 8, because they all share
> `QWEN36_RC_CANONICAL = (7, 2)`. **A limit that cannot be widened by environment is a
> different and better thing than a limit that merely happens not to be widened**, and an
> operator cannot produce a commitment wider than the court admits by setting a variable.

### Three times today the note existed and its author walked past it

Not three careless moments — one mechanism, and the most uncomfortable pattern of the evening
because every instance is self-observed within minutes of writing the rule:

```
"assert the report's verdict before reading any field"   -> KeyError 'verdict', one turn later
"pgrep -f matches the asker" (written 2026-08-30)        -> read `script: 2` as running; the
                                                            script had exited 20 min earlier
"check the arrow, not the fact" (1c, this hour)          -> "I have no GGUF, therefore I cannot
                                                            run the gateway smoke" — written
                                                            WHILE composing the rule; the A16
                                                            worker never reads MISAKA_PALW_GGUF
```

> **A rule is at its weakest immediately after you articulate it, not its strongest — because
> stating it feels like having applied it.** The satisfaction of having named the failure mode
> is the same feeling as having checked for it, and nothing distinguishes them from inside.

The practical form is not "try harder to remember". It is that **a rule earns its keep only
once it is attached to a mechanical trigger** — a script, a gate, a grep — because the moment
it is only in prose it competes with the feeling of already having used it. The three that did
fire today were all mechanical: `check-doc-citations.sh`, the anchor-count guard that refused
my `sed` when it found 1 occurrence instead of 2, and `strings | grep -c pow-agent`.

*The guard that stopped the bad `sed` is the smallest and the most instructive: it cost one
line, it fired within seconds, and what it caught was that my own pattern did not match my own
text.*

## The artifact the free-prompt lane needs, and two traps around it

On the merged tree the worker resolves the genesis class — `4277d84f7d91528c…`, n_ctx 512, from
the artifact header — so the width wall is gone and stopper (1) is closed. It then refuses at
boot for a new reason:

```
this artifact declares no tokenizer: `tokenizer_commitment` is all zeros, so every job this
class produces would publish `tokenizer_id` 0 and no replay could prove it read the ids this
class means. Re-convert it with a tokenizer bound … binding a tokenizer moves
`artifact_digest`, so this is a NEW ARTIFACT AND A GENESIS DECISION, not an upgrade
```

**The final clause is false for this row**, and the tree says so in two places:

```
misaka-palw-base0/src/classes.rs:1331
  assert_eq!(v5.artifact_root(&bound), v5_root,
    "a court-capable row registers the operand inventory, which the tokenizer is not in — so
     binding one is NOT a genesis input for this row and the registered root does not move")
ecc7cefb  "the inventory root is invariant under the graph and the binding (1a7457f1… for both)"
```

A court-capable row registers the **inventory root**, not the container digest. The remediation
text is right for a digest-rooted v1 row and **was never narrowed when court-capable rows
arrived**. Read literally it says the free-prompt answer path needs a genesis change, which
tonight means *not before launch*; read correctly it needs a conversion run and nothing else.

> **An error message that overstates the cost of its own fix**, on the one path that decides
> which sentence the launch document can carry. A limit rendered as a verdict — pointed at the
> operator rather than at us.

### Trap 1: the checkpoint on the build machine is the WRONG MODEL and looks right

```
Mac  models/qwen2.5-1.5b/model.safetensors   sha a961db72…  HF Qwen/Qwen2.5-1.5B  (BASE)   config 684 B
shipped qwen25-1.5b-a16.palwart              from Qwen2.5-1.5B-INSTRUCT dd924a11…          config 660 B
```

**Same file size (3,087,467,144 B) and the same `tokenizer.json` — commitment `fa9a4352…a649bb`
identical for both.** Size and tokenizer cannot tell them apart. A re-conversion from the Mac
checkpoint lands ~484,183,979 bytes different and produces a different class. **This was once
diagnosed as "the build cannot reproduce the shipped artifact"; the converter is deterministic
and innocent, and the input was the difference.** `qwen25-convert` on this machine is the one
command that must not be run.

### Trap 2: a file whose NAME asserts what its digest denies

Four files on ibm, measured:

```
qwen25-1.5b-a16.palwart          1795427276 B  sha a8c4e53e…  2026-08-27   <- shipped
qwen25-1.5b-a16.rebound.palwart  1795427276 B  sha a8c4e53e…  2026-09-03   <- BYTE-IDENTICAL
bound-candidate.palwart          1795427276 B  sha 3f8fc506…  2026-09-03 08:45
```

**`rebound` is a copy.** Its name says a rebind happened; its digest says one did not. It sits
beside the artifact it claims to derive from, and anyone reaching for the obviously-named file
gets the unbound artifact and the same all-zeros refusal — twice, because the second failure
looks like the first.

> **A name is a claim, and nothing verifies it.** Same shape as `#[ignore = "passes"]`, as a
> group of three tests named after the one file that holds one of them, and as a `--report`
> whose field set implies a verdict it does not carry. **This project's most durable defect is
> not a wrong value; it is a true-sounding name attached to something nobody re-derived.**

### What `bound-candidate` actually is — measured, and not what I assumed

```
cmp -l shipped bound-candidate
  differing bytes  128
  first offset     1,777,209,033      98.985% through the file
  last offset      1,795,427,276      = EOF
  span             18,218,244 bytes   = 17.4 MiB
```

**The 1.777 GB of weights are byte-identical**, so this is not a re-conversion from another
checkpoint — that would differ by ~484 million bytes. But the 128 differing bytes are **not
contiguous**: they are scattered across the final 17.4 MiB. A tokenizer commitment is 32 or 64
contiguous bytes. **A count and a location answer different questions and I had only asked for
the count.**

So the byte diff is *encouraging and insufficient*. The decisive measure is
`a16_root_probe::print_a16_v5_root_forms` over the two real files — `artifact_digest` must move
and `inventory_root(v5)` must not. **A small diff says how different; only the inventory root
says whether the class moves**, which is the property that decides whether this artifact can be
registered against the genesis being cut. Running on ibm (`8923b354`, not the cut tree).

*The probe is the third instrument the 62-ignored set has supplied tonight* — after the IBD
participation trio and the drill goldens. **The enumeration has now paid for itself more times
than it cost**, which is worth saying given it began as a risk about what a skip hides.

### `test result: ok`, `RC=0`, and zero tests ran — mine, tonight

```
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s
RC=0
```

ibm's checkout is `8923b354` and its copy of the probe holds `print_a16_root_forms`.
`print_a16_v5_root_forms` exists only on `palw-adr0082-impl`. **My name filter matched nothing,
so cargo filtered out both tests and reported success.** Four minutes of compile, then nothing,
then the word *ok*.

The only tells were `2 filtered out` and **`0.00s`** — a probe that maps two 1.79 GB artifacts
and hashes their inventories cannot finish in zero. *The wall clock is the evidence a test ran*,
for the third time today, and this time against a run I designed.

> I verified the env var names against the test source. **I did not verify that the test name
> existed in the tree I was running it on** — I checked the arguments and not the target. That
> is the arrow rule pointed at a command line: *"the probe exists, therefore this invocation
> runs it"* is an inference, and the tree it runs on is the thing that has to be true for it.

### The checker I wrote to catch silent absence had two silent absences

`scripts/check-doc-tree-claims.sh` reports which identifiers a document mentions whose
**definition differs between two trees** — the class `check-doc-citations.sh` cannot see, because
a claim with no `file.rs:NNNN` has nothing to resolve. It took three runs to work, and every
failure printed a clean plausible number:

```
run 1   mapfile: command not found          macOS /bin/bash is 3.2 — a script that runs on
                                            the fleet and dies on the release machine
run 2   "1 identifier, not found"           the array became a string and `"${idents[@]}"`
                                            expanded to ONE element, over a document with 299.
                                            A loop over the wrong collection reports a small
                                            clean number, NOT a failure
run 3   "0 found in either tree"  (x39)     `\b` in `git grep -E` is POSIX ERE: it matches
                                            NOTHING and reports zero. And removing it makes
                                            `MODEL_ID` match `MODEL_IDENTITY_KEY` — the naive
                                            repair trades a false zero for a false hit
```

**A tool written to catch absence-you-cannot-see, silently reporting an absence it could not
see, twice.** Only running it against a known-present identifier found either.

Working, it earns its place on its first real run — **six identifiers this card names whose
definition differs between the trees it is written across**:

```
PALW_STATE_V2_VERSION           5f  = 18                     impl = 20
genesis_anchored_v1             5f  (profile)                impl (profile, ladder)   <- gained an argument
PALW_LADDER_FAMILIES_V1         5f  <absent>                 impl  [PalwLadderFamilyV1; 2]
print_a16_v5_root_forms         5f  <absent>                 impl  present            <- tonight's probe
MODEL_ID                        same LINE, different FILES   (a16 worker vs qwen36 worker)
```

`genesis_anchored_v1` is the sharpest: **it takes a `ladder` argument on impl and does not on
5f.** Every sentence in this card about that wrapper's cost model is a sentence about one of two
different functions, and until now none of them said which.

*The `MODEL_ID` row is the tool being honest about its own resolution*: the definition line is
byte-identical on both trees because two different files each define a `MODEL_ID`, and a
leaf-name match cannot tell them apart. It is reported as differing-or-not by line, which is the
right conservative answer and not the whole one.

## §1's `palw_context_ladder` row: the conclusion may stand, the REASON no longer does

§1 says `palw_context_ladder` → **`None`, DO NOT ARM**, and gives as its reason:

> *"There is no `palw_context_ladder_active_at`: nothing reads the fence. Every use of that name
> is the module, the field, the `never()→None` normalisation, or `params.rs:3243` writing it into
> the FINGERPRINT. So arming it moves the fingerprint and gates nothing."*

**On `palw-testnet-5f` that is exactly true** — measured, not recalled: outside `config/params.rs`
the only reads are the guard assertions in `palw_context_ladder.rs`. **On `palw-adr0082-impl` it
stopped being true tonight**, in 5b's preflight fix `e5651de0`:

```rust
// consensus/core/src/palw_class_admission_v2.rs   (ADDED in e5651de0)
let ladder = params
    .palw_context_ladder … .then(|| palw_class_ladder_rules_for_court_v1(profile, court, bundle.court.max_step_leaf_count()))
```

**That accessor is the fence's first real read.** The row's conclusion is probably still right —
§1 records FG proving the 512 row admits and prices at 2^26 *with the fence dormant* — but
**a decision whose premise stopped applying is not automatically wrong; it is merely no longer
supported by what it says.** The row keeps its verdict and loses its argument until re-derived.

### And the presets now diverge, deliberately, with a test that says so

```
DEVNET_PARAMS             palw_context_ladder: Some(ForkActivation::always())
palw_rc_shipped_params    None                                                <- t11, the cut
mainnet / testnet / simnet   None
```

`only_devnet_arms_the_context_ladder` asserts exactly this, and the arming site's comment cites
**this card's §1** for the two-moves rule. It is considered, not accidental. On 5f the same test
asserted `devnet_shipped_params().palw_context_ladder.is_none()`; that assertion is gone on impl
and replaced by an equality against the armed value. *The guard did not silently break — it was
rewritten to state the new truth, which is the right way for a guard to change.*

> **The open question, and it is the one the announcement leans on:** 5b's devnet drill validates
> the graph-v5@512 registration under `ladder: Some(…)`. **t11 registers it under `ladder: None`.**
> Same call, different admission shape. This is the defect 5b just fixed, one level up — *the
> pre-check asked a court the chain does not run; the drill asks a network the chain is not.*
>
> Asked, not assumed: **is that registration exercised anywhere under `palw_rc_shipped_params()`?**
> Either answer is fine tonight — a covered case to cite, or a gap to state — but it must not be
> *inferred* from a green on the other preset.

*Recorded because it is the third time today that the thing being validated and the thing being
shipped were configured differently, and the first two were only found by looking.*

## ANSWERED: binding a tokenizer does not move the registered root

Run on ibm against the two real 1.79 GB artifacts, `88.55 s`, `1 passed`, **`0 filtered out`**:

```
[shipped] artifact_digest       c00faa480f2344d4a737e5b2e87ab606…
[bound]   artifact_digest       158314b58843430efebe343d61d1078c…    <- DIFFERS
[shipped] inventory_root(v5)    1a7457f100d9fb0f3406d882b4b5bcd7…
[bound]   inventory_root(v5)    1a7457f100d9fb0f3406d882b4b5bcd7…    <- IDENTICAL
shipped inventory(v5) == bound inventory(v5)   true
v5 == the v2 genesis pin                       true  (both files)
v5 profile class id  4277d84f7d91528c…    v2 profile class id  71bbb75513cf3d47…
```

**`bound-candidate.palwart` is registrable against the genesis being cut.** The worker's refusal
text — *"binding a tokenizer moves `artifact_digest`, so this is a new artifact and a genesis
decision, not an upgrade"* — is **false for this row**, confirmed on the two real artifacts
rather than on a synthetic `with_tokenizer_commitment`.

**Stated precisely, because two equalities print adjacent and only one is being claimed:**
`v5==v2 inventory` is `true` for both files, so the inventory root does not distinguish the
graphs. The claim is *"binding a tokenizer does not move the **v5** inventory root"* — the same
profile across two files. **It says nothing about v5 versus v2.**

`c00faa48…` now agrees three ways: the trailing 64 bytes read directly out of the file, the
digest the worker printed in its refusal, and this probe recomputing it. Three routes, three
machines, one value.

### The run before this one said `ok` in 0.00 s

Both invocations are in this card and the pair is the lesson:

```
0.00s   ok. 0 passed; 0 failed; 2 filtered out    RC=0   <- nothing ran
88.55s  ok. 1 passed; 0 failed; 0 filtered out    RC=0   <- read 3.6 GB
```

**Same word, same exit code.** `0 filtered out` and the wall clock are the entire difference.
*A targeted test invocation must state how many tests it expected to run*, because
`0 passed, 0 failed` is the one result meaning the **command** was wrong rather than the code —
and `cargo test` spells it with the word `ok`. (`cargo nextest` refuses it by default.)

### Getting the probe to a machine that had the artifacts

ibm's `origin` is GitHub and `palw-adr0082-impl` has never been pushed, so ibm could not fetch
the tree. Instead: **check that all five of the probe's dependencies exist on `8923b354`**
(`palw_a16_context_row_profile_v5`, `qwen25_a16_profile_v2`, `decode_artifact_file_v1`,
`a16_inventory_v1`, `PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT`) — they do — copy the file, watch
it fail to compile because **three other tests in it** want impl-only symbols, trim to the 55
lines that are the probe, run that. **12 KB of network against the 1.79 GB that copying the
artifact would have cost.** The copied file was removed afterwards; ibm's checkout is clean.

## LAUNCH-STOPPER, found by asking: t11 would have admitted the fused row at genesis and refused it from everyone after

The question in the previous section — *"is that registration exercised under
`palw_rc_shipped_params()`?"* — was answered by writing the test, and it is **RED**:

```
under palw_rc_shipped_params()   (court ARMED, palw_context_ladder DORMANT)
verify_class_admission_v6 refuses the graph-v5 row:
    PricedForADifferentCourt — "the class is priced for a dissection of None
                                and this ruleset's court plays 2"
```

**Mechanism.** v6 derives the priced cost shape as
`ladder.map_or_else(genesis_anchored, |r| r.cost_shape)`, and the processor's registration arm
passes `ladder: None` whenever the **context-ladder** fence is dormant. So on t11 a fused
profile gets the genesis-anchored shape (dissection `None`) and the court check refuses it.

**The genesis route never goes through v6.** `qwen25_a16_graph_v5_registration_v1` calls
`palw_class_ladder_rules_for_court_v1(profile, Some(court), ladder)` unconditionally and prices
the row *for the court*.

> **So the chain we were about to cut would register the fused class at genesis and refuse the
> same class from every later applicant.** ADR-0054's permissionless admission, the demonstration
> class, the whole *"anyone can bring a model"* sentence — refused by the gate on the shipped
> preset, while devnet says yes. **The genesis set would have worked perfectly and the chain
> would have been closed to newcomers**, which is the failure that looks healthiest from inside.

Devnet admits (ladder armed), t11 refuses. *The drill asks a network the chain is not* — and the
three near-misses 5b enumerated were all one step away from the shipped case:

```
the genesis route          prices from the bundle regardless of any fence — bypasses the gate
armed_rulesets test        passes Some(rules) for BOTH devnet and rc — never asks with None
the new SDK test           runs on devnet
```

### Why this rule change can ship tonight, and only tonight

The fix is a **consensus rule change on the acceptance path**. It is safe *because we are
re-genesising*: the behaviour it modifies is currently **"always refuse"**, so no chain holds a
post-genesis fused registration the new rule would re-judge, and every node starts from the new
genesis running the new rule. **The same change on a live chain would need a fence.**

That reasoning depends on the wipe being complete. **A missed host is now not merely a stale peer
but a node judging registrations by a different rule** — which raises the price of the wipe
defects §4b already lists, and is one more reason `verify` must be clean before `wipe` runs.

### The test to write is route-agreement, not fence-invariance

The proposed test is *"the gate prices a fused row identically with the ladder fence armed or
dormant."* **That pins fence-invariance, and fence-invariance was not the defect.** Two *routes*
priced the same row differently and nothing compared them; a fence-invariance test passes if both
routes are wrong in the same way, and it would have passed on `e5651de0` had v6 simply ignored
the fence and stayed genesis-anchored.

```
the property that broke:
    price( GENESIS assembly, row, bundle, court )  ==  price( v6, row, bundle, court )
fence-invariance is a COROLLARY of that, not a substitute for it
```

> **Third instance tonight of one object spelled twice with no equality between the spellings:**
> the preflight asked a court the chain does not run; the drill asked a network the chain is not;
> the genesis assembly and the acceptance gate price the same row by two different functions.
> **This project's most-repeated defect, and each time the two spellings were individually
> correct.**

### Confirmed twice, from two layers, by two people who did not share code

```
1c   consensus-core, verify_class_admission_v6 direct   PricedForADifferentCourt { priced: None, court: 2 }
5b   the SDK preflight, as the panel calls it           "priced for a dissection of None … court plays 2"
```

**Two entry points, no shared code between the calls, the same refusal.** The two-ways rule
delivered its answer on the load-bearing claim and the answer was red. *The thing I most wanted
to be a false alarm is now the hardest to dismiss.*

**Scope, narrowed to what is actually true** — my first phrasing above ("closed to newcomers")
is too broad and is corrected here rather than edited away:

```
genesis registration of the fused row      FINE — the genesis route prices from the bundle,
                                                  bypassing the gate entirely
post-genesis registration, NON-fused       FINE — keeps the genesis-anchored shape, unaffected
post-genesis registration, FUSED, on t11   REFUSED
```

So the floor and the non-fused rows still admit. **What is shut is precisely the family the
network is being minted around**, which is narrower than what I wrote and quite bad enough.

**And §1's ladder row takes its final form** — not *arm it*, not *leave it and all is well*:

> **Leave it dormant — and know that post-genesis fused registration does not preflight under
> t11's own shape.** Arming remains wrong (it moves t11 to the shape nothing has validated for
> t11's registered set). What changed is that t11's shape now has a **measured consequence**
> rather than being merely the untested alternative's opposite.

*How 1c avoided a false alarm at the worst hour is worth copying:* they did not re-verify that
the row is fused or that the preset was right. They verified **that their invocation asks what
the panel asks** — `palw_panel.rs` → `palw_admission_shape_at_v1` → `preflight_admission_with_chain`'s
own doc saying it probes v6 with *exactly those* inputs. **Checking the arrow, on their own
result, against themselves** — which is the failure mode that has bitten three times tonight.

## §4b, fourth defect: the wipe census invented one appdir and lost a running node

Found by re-running the census after the launch-stopper made a surviving host expensive — a
missed node is no longer a stale peer, it is **a node judging registrations by a different rule.**

**Three forms of the same enumeration, wrong in both directions:**

```
pgrep -af kaspad | grep -oE "appdir=[^ ]+"
    -> printed `appdir=[^` on ALL THREE HOSTS, for as long as this script has existed.
       The remote shell's own argv holds both "kaspad" and the literal regex, so the grep
       extracted its own pattern out of itself. THIRD instance tonight of `pgrep -f`
       answering the asker — and this one is in the wipe script.

pgrep -x kaspad
    -> the repair. It dropped `kaspad.candidate` on ibm — a RUNNING NODE on /tmp/fpchk.
       **A false positive repaired into a false negative**, which is the direction that
       loses a host on wipe night.

walk /proc, match the EXECUTABLE's basename by prefix
    -> correct. The asker's exe is `bash`, so it cannot appear; any `kaspad*` build does,
       whatever it is called.
```

**The corrected census — seven running nodes, not five and not six:**

```
169.58.39.220    kaspad                       --appdir=/root/.t11
                 kaspad                       --appdir=/root/.t11b
                 kaspad.candidate (deleted)   --appdir=/tmp/fpchk      <- invisible to both
                                                                          earlier forms
169.58.232.113   kaspad                       --appdir=/root/.t11
                 kaspad                       --appdir=/root/.t11c
                 kaspad                       --appdir=/var/lib/misaka-minerpool/slots/slot-01/appdir
5.104.81.23      kaspad                       --appdir=/root/.t11
```

**`(deleted)`** is the third thing the new form reports and neither old one could: that binary
has been **unlinked from disk while the process runs.** Nothing can be read off it, and it will
not come back after a kill — which is convenient here and would be alarming anywhere else.

> **A repair aimed at a false positive should be checked for the false negative it creates.**
> `[^` in a wipe list is noise; a missing node is the whole failure. The check that caught it was
> noticing that `/tmp/fpchk` **disappeared between two runs** and asking why, rather than reading
> the tidier output as the better one. *The output got cleaner and less true, and clean is what
> a repair is supposed to look like.*

### The ceremony's own guard grepped the tree for the answer

`scripts/check-repin-predictions.sh`, first version, reported **all four values as DIFF** against
`3f9526d3`. It read `<empty>` for every one, because none of the five predicted values appears
anywhere in the tree's `.rs` files — and **they should not**:

> **These values are what the re-pin WRITES. The tree legitimately holds the OLD pins until the
> paste.** A guard that greps the source for the answer is asking the tree to already contain the
> thing the ceremony exists to put there.

It stopped, which was right, **and its reason was wrong** — it said *different* where the truth
was *absent*. Safe exactly once, misleading every other time, and it would have said the same
four DIFFs if the predictions had been perfect.

Rewritten: `derive/src` is verified from the tree (it is a property of the tree, so that half was
always sound and reads **OK** on `3f9526d3`), and the five values are compared against an
**extraction table** — `<name> <value>` lines produced by the thing that computes them. With no
table it prints what it cannot answer and exits, rather than manufacturing a verdict:

```
TABLE=/path/to/extracted-pins.txt scripts/check-repin-predictions.sh palw-adr0082-impl

  source_tree_sha256  637858dba5ea5e34…
  t11_fingerprint     71efa66480211731…
  devnet_fingerprint  c0da0c9024d68b94…
  fp_golden           c940b5c36ee40846…
  premine_builds      ba2612417e7e0817
```

And it now distinguishes the two failures by name: **`<absent>` means the check did not run;
a differing hex means it ran and disagreed.** Conflating those is what the first version did.

*Four hours after writing "a rule earns its keep once it is attached to a mechanical trigger",
the trigger I attached was broken in the way the prose version would not have been — a human
following the checklist would have run the extractor first, because a human knows the pins are
not there yet.* **Mechanising a rule can lose the context that made the rule obvious.**

### Correcting my own safety argument: the timing is the other way round

I wrote that the v6 change *"is safe because we re-genesis, and the same change on a live chain
would need a fence"*, and that a missed host therefore *"judges registrations by a different
rule"*. **The conclusion is right and the timing is inverted.**

**The defect predates tonight.** `processor.rs:6161` — the acceptance path — computes the ladder
from the fence exactly as the preflight does:

```rust
let ladder = self.palw_context_ladder_at(point.daa_score).then(|| {
    palw_class_ladder_rules_for_court_v1(&carriage.profile, court, bundle.court.max_step_leaf_count())
});
verify_class_admission_v6(bundle, &carriage.profile, &carriage.canonical, object, …, ladder.flatten(), court, false)
```

and **`e5651de0` did not touch the processor** — its five files are `palw_class_admission_v2.rs`,
`palw_panel.rs` and three SDK files. So the chain has refused post-genesis fused registrations on
a ladder-dormant preset **for as long as the processor has had that arm.**

> What `e5651de0` changed is that the preflight now **agrees** with the chain. Before it, the
> preflight refused for a *different* reason (no court) — **and the disagreement hid the rule.**
> Two wrong answers that differ look like one bug in the newer component.

**So the wipe's status is:**

```
TODAY, before the fix      every node runs the same processor and refuses identically.
                           No divergence. A surviving host is an ordinary stale peer.
AFTER the fix ships        the acceptance rule differs between fixed and unfixed nodes.
                           A missed host judges the same object differently.
```

**The wipe becomes load-bearing for consensus when the fix ships, not now.** My sentence was
right about the danger and wrong about when it starts — which matters, because it is the
difference between "the wipe was already critical" and "the wipe becomes critical at a moment we
control and can therefore sequence."

### And the corollary that must gate any candidate fix

**A preflight-only repair turns both new tests green while the chain still refuses.** 1c's test
asks v6 directly and 5b's asks the SDK, and *both* would pass over a closed door if someone
patched `palw_admission_shape_at_v1` alone. **Checked, not assumed** — `97bdbf75` changes
`verify_class_admission_v6`'s own body:

```rust
-  let shape = ladder.map_or_else(|| genesis_anchored_v1(profile, bundle_ladder), |r| r.cost_shape);
+  let shape = match ladder {
+      Some(rules) => rules.cost_shape,
+      None if fused && court armed =>
+          palw_class_ladder_rules_for_court_v1(profile, court, bundle_ladder)
+              .map_or_else(|| genesis_anchored_v1(profile, bundle_ladder), |r| r.cost_shape),
+      None => genesis_anchored_v1(profile, bundle_ladder),
+  };
```

That is the function `processor.rs:6161` calls, so the acceptance path moves with it.
**Any later candidate fix gets the same check before it is believed.**

### 4,223,328 is not a stale figure — it is a quoted failure, read as a fact

The graph-v5@512 row's canonical count at the ruleset's `2^26` ladder is **6,630,544 leaves**.
An earlier assertion used **4,223,328** and the gate corrected it. That was reported to me as *"an
old SDK comment says 4,223,328 — that figure is stale."* **It is not stale.** Both places that
carry it label it as the wrong answer, and one of them prints the right one three lines above:

```
consensus/core/src/palw_class_admission_v2.rs:1891
    /// On the graph-v5 512 row the honest count is 6,630,544 and the helper answered
    /// `"the canonical job does not count against this profile: job shape yields 4223328 step
    /// leaves, exceeding the 4194304 cap"`, so the object could not be BUILT at all
```

The comment is **correct, complete, and adjacent to the truth**. The number was taken out of a
sentence whose subject is *that this number is wrong.*

> **This is the same near-miss as pasting the eight transformer ids out of a drill whose verdict
> was `FAILS`** — a value read from a context that says the value is wrong. **The context was not
> missing and it was not misleading. It was simply not part of what got copied**, and a hex string
> or a leaf count carries no trace of the sentence it came from.
>
> The defect is not a stale comment. **It is that quoting is lossy in exactly the direction that
> matters**: the number survives the copy and the word *"wrong"* does not.

*Recorded because "the comment is stale" and "the comment says this is the wrong answer" call for
opposite repairs.* The first would have someone edit or delete a correct comment — deleting the
only place the failure mode is written down, which is the precise repair that makes the next
occurrence unfindable. **Nothing here needs fixing except how the number was read.**

### The close figure, confirmed under the shape that ships — and a fourth value that never got in

5b's v6 fix verified from the third angle, green, and it printed the byte count as a side effect:

```
RC admission shape @ daa 0:  court arity 2, Flat ids, window 3000  |  ladder None (fence dormant)
t11 shape admission: class 4277d84f7d91528c admitted, close 81,312 B, 35,840 terminal MACs
test result: ok. 2 passed; 0 failed
```

**81,312 B is now measured from the genesis-registered carriage, priced under the shape the
acceptance path actually uses** — which is what §3 already said and had not yet been measured that
way. The figure has moved three times (81,599 → 82,719 → 81,312) and this is the first reading
taken under the configuration that ships.

**A fourth value existed and never reached either document.** 1c measured **83,175** and reported
it as the reproducible one, then found both of their measurements passed `ladder: Some(rules)` —
their `armed_rulesets` helper supplies it for *both* presets, and their genesis-object probe built
the rules itself and priced with them. **t11's actual shape is `ladder: None`.**

> They corrected my arity, corrected my ids form, and then priced under the wrong **ladder** —
> **the third field of the same three-field shape, and the one they had just built a test to
> expose.** Two fields read off the bundle, one supplied by the caller, and a number that looks
> derived. *That is the `kary_court` defect they named, committed by the person who named it.*

**And the announcement's decision survives its fourth number.** It carries **no byte count at
all** — one carrier and a real command instead. Checked mechanically just now: `83,175`, `83175`,
`81312` and `81,312` all appear **zero** times in
`docs/testnet11-relaunch-5f-announcement-draft.md`. *A figure that has moved four times under
three configurations is not a figure a launch document should carry, and the decision to drop it
was made before three of those movements were known.*

**What made this visible was the red test, not care.** It was written to check someone else's
rule, and the first thing it printed on going green was a correction to its author's own
arithmetic. **A test that asks the shipped question answers more than the question it was written
for.**

### The ceremony guard, exercised in all four directions before it is needed

A guard that has only been seen to refuse is indistinguishable from a guard that always
refuses. All four paths run, on real trees:

```
1  no `extracted_from` line          STOP  "five values from an unnamed run prove nothing"
2  all five values CORRECT, but      STOP  table says   4b0e15675c07ec1d…
   extracted from 4b0e1567 while           checking     3f9526d343892a48…  (palw-adr0082-impl)
   checking 3f9526d3                       "values from another run are not evidence about
                                            this one, however right they look"
3  correct table, correct tree       PASS  all five predictions hold
4  one value mutated                 STOP  DIFF t11 fingerprint, both values printed
```

**Case 2 is the one worth having built.** Every value in that table is correct — it is the same
five hex strings the ceremony expects — and the guard refuses anyway, because *correct values
from the wrong run are not evidence.* Before this the guard would have printed `all five
predictions hold` over exactly that table, which is how it was found: **run green against a
`4b0e1567` table while checking `3f9526d3`.**

5b wired the emitter to take `extracted_from` from **the finalize log's own closing `impl head:`
line, never from the current HEAD**, and to exit 2 without exactly one such line. *That is the
join belonging to the run rather than to the typist* — a table cannot claim a tree it was not
computed on, even by accident, and the failure mode where someone regenerates the header without
regenerating the values is closed by construction.

> **The discipline is "has it been seen to fail AND to pass", and case 2 says that is not enough
> either.** This guard passed on a table it should have refused, and every value in it was right.
> *Seeing it pass is only evidence if you also know what it would have refused.*

## The MIDI and the 3D, produced and read by something that could have disagreed

The goal names *midi / 3D / 実用的な出力*. I had been carrying that half on someone else's
report. Produced here, on `palw-testnet-5f`, and checked three ways:

```
palw-derive derive --transformer music/smf/v1 --answer corpus/music/03-overlapping-melody.json
                   scene/glb/v1               corpus/scene/02-hierarchy.json
                   cad/stl/v1                 corpus/cad/01-extrude-l-bracket.json
```

**1 — the files exist and carry content**, parsed by `scripts/verify-artifacts-independently.py`,
which implements SMF, glTF-binary and binary-STL **from their published specifications** and
imports, links and shells out to nothing in this tree:

```
OK   .mid   157 B   SMF format 1, 2 tracks, division 192, 9 note-on, 1728 ticks
OK   .glb  2736 B   glTF 2.0, 2 chunks, 3 nodes, 2 meshes, 2 primitives
OK   .stl  1084 B   binary STL, 20 triangles, no degenerate facets
```

It walks every MIDI event and checks each track's declared length against where its events
actually end, checks every glTF accessor fits its bufferView and every bufferView its buffer, and
checks the STL triangle count implies exactly the file's length with no two vertices equal.
**These are the checks that fail on a plausible-looking blob**, which is the only kind worth
running.

**2 — the bytes are the ones the chain pins.** Each artifact hash equals its `golden.json` entry:

```
MATCH  music/03-overlapping-melody   6e27611c2ee15af0…   157 B
MATCH  scene/02-hierarchy            6da65dbc8f6a29fa…  2736 B
MATCH  cad/01-extrude-l-bracket      8b6c0d4389cb20cb…  1084 B
```

> **So the bytes the chain commits to are files an independent reader accepts** — which is a
> different claim from the drill's, and the one an operator cares about. `drill --check` compares
> one run's hashes against another's: it proves two runs agree, **not that the output is a file
> anything else can open.** Two runs of a broken writer agree perfectly.

**3 — the ids here are 5f's, and my re-pin predictions are impl's.** Stated because the two sets
look interchangeable and are not:

```
palw-testnet-5f     derive/src = 6b8d13ad46ebb22c…   music/smf id 4f4edd02c53ae28e
palw-adr0082-impl   derive/src = 4969f8dc051cac31…   music/smf id cb5f27b4… (predicted)
```

**The trees differ, so the ids differ**, and `transformer_id` is a function of `derive/src`'s
bytes. *A transformer id read from the wrong tree is exactly the paste this ceremony exists to
prevent* — and it would have been eight plausible hex strings.

## A valid drill on 5f — and three things it settles

```
palw-derive drill --corpus misaka-palw-derive/corpus --report drill-5f.json
{"arch":"aarch64","golden_checked":43,"golden_mismatched":[],"golden_unpinned":[],
 "bounds_enforced":8,"bounds_not_enforced":[],"refused":10,"rows":33,
 "verdict":"the corpus reproduces its goldens on this architecture and every declared bound
            refused an over-bound answer"}                                          exit 0
```

Run with **both binaries built** (`cargo build -p misaka-palw-derive`, not `--bin`), so
`palw-evm-runner` is beside the deriver and the six EVM goldens are checked rather than refused —
the defect that once produced eight plausible ids from a `FAILS` run.

**1. The report file still has no `verdict`, confirmed on a run that PASSED.** Previously I had
only compared a valid report against a failed one; this is a third, independent confirmation from
a fresh green run:

```
file keys   arch bounds golden os refused rows schema source_tree_sha256 transformers
verdict in the file      False        <- stdout has it; the artefact does not
golden.mismatched == []  True         <- the discriminator
```

**2. The report schema itself differs between trees, which my own rule did not say.** I recorded
the key set as *"…transformers **uncovered**"* and asserted `uncovered == []` is empty in both a
valid and a failed report. **On 5f there is no `uncovered` key at all** — so `r.get("uncovered")
== []` is *False* here, on a perfectly good run. That measurement was taken on impl.

> **A rule about a report's fields is a claim about a tree**, and mine did not name one. The rule
> survives — `golden.mismatched == []` is the discriminator on both — but *the sentence I wrote
> around it was tree-specific and did not say so.* Third time tonight for the same omission, and
> this one is inside the correction I made to a previous omission.

**3. `source_tree_sha256` tracks the tree exactly as the prediction assumes.** 5f's drill reports
`98265872fb7a372c…`; my re-pin prediction is `637858dba5ea5e34…` for impl:

```
palw-testnet-5f     derive/src 6b8d13ad46ebb22c…   source_tree_sha256 98265872fb7a372c…
palw-adr0082-impl   derive/src 4969f8dc051cac31…   source_tree_sha256 637858dba5ea5e34…  (predicted)
```

**Two different trees produce two different values, and neither is a coincidence** — which is the
mechanism the whole prediction rests on, demonstrated rather than assumed. *The best evidence
that a pin tracks its input is watching it move when the input does.*

## The free prompt answered — on devnet, and the stage after it failed for a script reason

The devnet e2e on `e5651de0` reached **stages 1–5**: the class registered on chain, the family
certified, the gateway healthy, and **one chat answered.** Then 5b:

```
the gateway queued no commitment … this class is not seated on the free-prompt lane
(fp_certified false)
node logs: ClassLaneCertified binding 0b8a6c35… DROPPED on all three nodes —
           "no family certified on this chain for the free-prompt lane covers every kernel
            class 4277d84f…"
```

**The chain refused correctly.** Stage 3 of `scripts/misaka-palw-fp-devnet-e2e.sh` ran
`palw-certify drill --family a16` — the **graph-v2** family `PALW-QWEN25-A16` — while the row
under test is **graph-v5@512**, whose kernels are `PALW-QWEN25-A16-V5`'s. The `bind` output says
so in as many words: *"covered by the PALW-QWEN25-A16-V5 RC family."*

> **A script naming a family by hand where the catalog row already names it** — one spelling
> against the row's — and the chain caught the difference by refusing a binding whose coverage
> did not reach the class. Fixed at `21a34935` by deriving the family from the model id *the way
> `bind` does*. **This project's most-repeated defect, now at seven instances**, and the only one
> so far where the chain itself was the thing that noticed.

### `drill exit 0` while the script had died at stage 5b

5b's launcher reported success for a run that failed. **The verdict was in the log and not in
the exit code** — the same shape as `test result: ok` over zero tests, and the same shape as a
`--report` that carries ids without carrying whether the run was valid.

```
tonight's exit-code-and-verdict disagreements, all three found by reading the log:
   cargo test  "ok" in 0.00s, 2 filtered out, RC=0      nothing ran
   palw-derive drill --report                            file has no verdict at all
   the devnet e2e launcher   "drill exit 0"              the script died at 5b
```

**Three different tools, three different mechanisms, one habit that catches all of them:** read
the log, and check the wall clock against what the work should have cost.

### What this does and does not say about the goal's free-prompt item

**It answered a chat.** That is the first time the free-prompt path has produced an answer end to
end in this cut's lineage, and it is worth stating plainly — **on devnet, on `e5651de0`, with the
ladder armed.** It is not the t11 shape, it is not the cut tree, and the stage that binds the
answer to the chain is the one that failed. **The announcement's sentence still has to come from
a run on the preset that ships.**
