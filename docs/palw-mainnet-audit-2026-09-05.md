# PALW mainnet audit, 2026-09-05 — the carded-mainnet pass

Scope: the code the live public testnet-11 fleet runs — Relaunch 5f plus the ADR-0084 Decision 7
line (`palw-adr0084-served-answer` at `32c772cc`), which is 947 commits ahead of `main`. The
question asked of every candidate finding was the one the 2026-08-30 audit asked:

> **Would this hurt on a mainnet carrying real value?**

"Mainnet" here means the world where `PALW_MAINNET_GENESIS_ARTIFACT_ROOT` and
`PALW_MAINNET_GENESIS_BONDS` are set: mainnet assembles the RC ruleset (ADR-0042 D11) and the
chain carries value. No previous audit had read that path — the last full one is dated
2026-08-30, and 1,223 commits and ~152k lines of Rust have landed since.

## Method, and what it cost

Eighteen adversarial finders, one per consensus dimension, over a survey of the carding path, the
prior audits' open items, the change surface and ADR-0080–0086. Each finding was then handed to
three refuters with distinct lenses (does the mechanism work / where is the guard / is it reachable
and does it pay), each told to default to *refuted*.

56 findings; 32 survived two of three refuters; 22 were refuted; 2 split. The refutation pass is
what makes the list below worth reading — it removed 40% of what the finders produced, including
several that were plausible and wrong.

**The fan-out was larger than it needed to be.** 168 verify agents (56 × 3) is roughly twice what
the yield justifies: most false findings die to the *guard* lens alone. A leaner shape — one
verifier per finding, escalating to three only for survivors and for anything CRITICAL — would have
reached the same list for about half the agents. Recorded here because the cost is real.

**Two method faults to read the results with.**

1. **The tree was edited while the audit read it.** Fixes landed mid-run, so a refuter that says
   "the guard exists at params.rs:1570" about the genesis-`bits` finding is reporting *this audit's
   own fix*, not a finding that was never real. Every item under "Fixed here" was verified by hand
   against the pre-fix tree before it was touched. Do not read those refutations as acquittals.
2. **52 agents were killed by a session limit** (50 verifiers, both synthesis agents). The
   dimensions whose verdicts are therefore missing are `host-security`, `freeprompt`, `time-daa`,
   `gates-and-tests`, `executor-determinism` and one third of `reward-mint`. Their findings are
   listed below as **unjudged** — substantiated by a finder, not yet attacked by a refuter.

---

## Fixed here

Eleven commits. Every one was checked failing before the fix where a test could express it.

### 1. A carded mainnet did not arm the fences testnet-11 arms — `c4b975bd`

`docs/adr/README.md` says a carded mainnet "assembles the RC's ruleset … with the same fences armed
from genesis". It did not. `mainnet_shipped_params` assembles over `MAINNET_PARAMS` — every PALW
fence `None` — and called `palw_rc_arm_phase1`, which arms three. The other three were literals on
`palw_rc_base_params`, a base mainnet never passes through:

* **`palw_difficulty_priced_rows` dormant while the heartbeat lane is armed from genesis.** That is
  precisely the state ADR-0083 exists to escape, and it is measured, not theoretical: on Relaunch 5f
  five heartbeat emitters tightened `bits` from p = 0.5 to p = 1.5e-3 in 826 blocks and no bonded
  lane could win again. Mainnet's difficulty window was 2.5× the RC's, so the wedge is deeper.
* **`palw_uncertified_weightless` dormant.** `validate_palw_v2` refuses to arm it above genesis
  ("it is genesis-only, or dormant"), so an uncertified class's pwu would have entered that chain's
  fork-choice weight **permanently**, and the free-prompt retirement hole — whose end state is
  `load_tip → CarriageInconsistent`, a node that cannot restart — would have been the chain's for life.
* **`palw_kary_court` dormant.** A card pinning ADR-0082's dense row did not ship a court it could
  not run; it refused to boot, and the card had no field with which to say the word.

The arming is now spelled once and every write is only-if-dormant, so it can add a fence a base
forgot and can never move one a base decided. testnet-11 keeps its stated DAA-1150 flag day; no
shipped fingerprint moves. The new test compares the fence sets as a **set**, from one table, so the
next fence added cannot be armed on one shipped network and forgotten on another.

Also closed in the same commit: **half a card is refused by name.** The ruleset reads the artifact
root and the premine reads the bond registry, and nothing cross-checked them — bonds without a root
mints seat collateral and fee floats out of the 10B cap for a `Disabled` mainnet that seats nobody.

### 2. A V2 network's difficulty window and parent count follow its cadence — `0048262e`

`palw_v2_params_on_base` imposes the 120 s cadence and re-derives the depths "in ONE place", for
the stated reason that a quantity restated per preset drifts on the preset nobody re-read. Two
quantities that are functions of that cadence were left behind:

* `difficulty_window_size`: mainnet's 661 is `2641 s / 4`, sized as ~264 **seconds** of memory at
  10 bps. At sample rate 1 and 120 s a block it is **22 hours** — a 300× change in the thing the
  constant was sized to be, arrived at by leaving it alone.
* `max_block_parents`: 16 rather than 10, because `with_two_minute_cadence` preserves a *deliberately*
  widened DAG and `Bps::<10>`'s default is not a deliberate widening.

A no-op on testnet-11 and devnet; the only ruleset it moves is a carded mainnet's. The residue is
asserted and named — `max_block_level` 225 vs 250, a block-level/pruning-proof property of the hash
lineage — so that difference is a decision on the record instead of a discovery after the mint.

### 3. A V2 genesis is minted at the ambient target — `187697f4`, `cc41350b`

ADR-0066: "a V2 network runs at MAX because the class lottery, not the hash target, is its
throttle." Both PALW presets mint at `0x207fffff`. `MAINNET_GENESIS` carries `0x1f7fffff` — one
compact exponent, 256× harder — inherited from the hash lineage. Carding is a re-mint, so `bits` is
a value the ceremony chooses, and nothing told it to choose this one: left alone, a carded mainnet
is born needing 256× the inferences per block for the whole 150-block fixed-difficulty launch
window, on the one network where "wait for the DAA to fix it" costs money.

`validate_palw_v2` now refuses it. The gate immediately found **49 virtual-processor fixtures**
rehearsing a network no carding ceremony could legally mint; they now mint correctly. Loosening the
gate to admit them would have been shaping the rule to agree with the fixture.

### 4. The coinbase output cap counts the mergeset — `ad984a83`

The cap was `ghostdag_k + 2 + 25`. ADR-0058 pays every entitled in-window **red** its own output,
and reds are bounded by the mergeset (180 on every V2 preset), not by `ghostdag_k`. A mergeset with
more than ~26 entitled reds makes the coinbase *this very node builds* fail its own isolation check
— the 112-block halt the function's own comment records, reachable by an ordinary wide DAG and
reachable on purpose. The payout is unfenced, so this is live on testnet-11 today.

**This one needs a coordinated restart**: it is a validity relaxation, and nothing in the
fingerprint separates an old build from a new one.

### 5. The fast grouped matmul checks every channel's exponents — `4b12a4bd`

`grouped_fast` validated by probing the reference with **channel 0 alone**, under a comment claiming
"the reference IS the validator … this path cannot admit anything the reference refuses". Every
other channel's exponents reached `acc += partial << *exp` unchecked. This workspace builds release
with `overflow-checks = true`, so a negative exponent **exits the process** — the exact failure the
reference's own "every exponent is checked, not the largest one" line was added to prevent,
reintroduced by the faster path beside it. The rule now has one spelling.

### 6. Host-side and gate repairs — `3f1e6e13`, `4959abcb`, `c85aa191`, and the gateway commit

* **ADR-0079 D4's seed-file guard looked for a shape this tree never writes.** It refused a boot on a
  32-byte raw seed; `misaka key gen` hex-encodes, so every seed file is 64 bytes. Both tests wrote
  the raw form — the guard passed on a fixture and would have missed the real mistake.
* **`expected_reds` labelled passing tests "known red".** It grepped for the test NAME, which
  cargo-nextest prints on its PASS line, and ran before the rc check — so a fully green run printed
  "known red: shipped_presets_have_pinned_fingerprints" every time. Its own comment says why that
  matters: "a reader who cannot tell a known red from a new one ends up ignoring the colour."
* **The gateway's request line and header lines were unbounded.** The body and header *count* were
  capped; each line was not, so one unauthenticated connection could grow a `String` until the
  public entrance died.
* **`--deny-purpose` named four of eight signing purposes.** The unnameable half is all of PALW,
  including the free-prompt quantum spend: a signer that must never spend could not say so.

### 7. Pinned, not fixed: ADR-0084 U-08 — `fafeda50`

The RC ruleset freezes `max_step_leaf_count = 2^26` and its genesis registers the dense graph-v5@512
row at 52,778,128 worst-case leaves, while the refutation path calls the **uncapped** leg entry
points at six production sites, all of which stop at 2^22. See the open list below; the test is the
alarm so the gap can neither close nor widen silently.

---

## Open — blocks a mainnet card

| # | What | Where |
|---|---|---|
| O-1 | **A dispute of the widest registered class ends `LeafCountOutOfRange` whatever the evidence says.** ADR-0084 U-08. `adjudicate_close_proof_v2` already holds `court: &PalwCourtParamsV2` and passes the executor constant instead of `court.max_step_leaf_count()`. Faked execution on that class is unconvictable — the one thing the court exists to prevent, and the price weight is charged for (ADR-0069). Rewiring changes which closes are valid, so it is an activation: `Params::palw_context_ladder` is the fence ADR-0077 Phase B reserved for it, and it gates nothing today. | `palw_step_refute.rs:2937, 2985, 3726, 3943`; `palw_step_leg.rs:1688, 1702` |
| O-2 | **`safe_frontier_blue_score` — fork choice's first key — is a carriage scalar nothing re-derives.** `into_state_v2` copies it verbatim; `assert_internal_consistency_v2` never mentions it (checked by hand: no occurrence in 4562–4950). Bound only by the state root, which the attacker authors on its own staged chain, so a first-sync against a hostile peer is pinned to that chain forever — the frontier is monotone and the designed bootstrap-recovery lane is itself defeated by it. Refuters corrected the reach: it needs a full fake headers-proof IBD and an un-raced commit, so it is an eclipse attack, not every first sync. | `palw_state_v2.rs:10305`, `ibd/flow.rs:1999` |
| O-3 | **The court is squattable.** A single global cap of `turn_deadline_daa` open sessions: fill it and every claim on the chain becomes un-challengeable. | `palw_state_v2.rs:8847` |
| O-4 | **Two ~60-byte `ObjectChunk`s per block permanently starve the certification lane.** The per-block grading slot is spent before the object is validated, and ADR-0075 SA-1's slot rent is dormant on a carded mainnet. Eight unsigned chunks squat all eight pending-chunk slots. | `processor.rs:5264, 5317`; `palw_state_v2.rs:8722` |
| O-5 | **`output_root` is a free field inside the priced attempt bytes.** ADR-0072 D8 says every field inside them is pinned; `output_root` is pinned by *replay*, which happens after the block has already won. One inference then buys many lottery draws, paid for only in escrow after the panel reacts. | `palw_attempt_v2.rs:945` |
| O-6 | **The panel's sortition randomness is a block hash the accused can re-roll at one hash per try** — the code's own comment says it costs one inference. Since ADR-0072 that is false, and ADR-0073 SA-1's p^k bound does not hold either. | `processor.rs:6507`; `palw_freeprompt_v3.rs:588` |
| O-7 | **The receipt lane pays no proof of work at all, yet its rows count as bits-priced.** A bondless actor retargets the attempt lane off its own chain — ADR-0083's repair, defeated through the other lane. Free receipt blocks also advance the DAA score, so every DAA-denominated PALW deadline can be run ~180× fast and courts won by clock. | `pow_layer0.rs:395`; `difficulty.rs:34` |
| O-8 | **On a fused-attention class the sweep clocks the responder at ladder-Terminal for an object no shipped binary constructs** and the graph-v5 row cannot produce, so silence convicts the honest producer. | `palw_state_v2.rs:6947` |

## Open — must fix before real value

| # | What | Where |
|---|---|---|
| O-9 | **With `palw_unavailable_abstains` armed no panel seat can be slashed for anything.** ADR-0065 D4 armed it to stop honest seats burning for transport loss; the cost is that a seat majority can void every producer's claim for free, and a void destroys the escrowed worker reward. The fix for one harm is the whole of another. | `palw_state_v2.rs:5941` |
| O-10 | **A panel seat is purchasable for 4 millionths of what the genesis registry paid** — the post-genesis bond floor is the fixture's 400,000 sompi against 10,000 KAS a genesis seat. Compounding O-9. | `palw_fp_devnet_v3.rs:438` |
| O-11 | **Cadence share is purchasable at 1‰ per 40,000 sompi of refundable collateral**; the aggregate-dilution guard protects only the base class and nothing bounds how many classes one bond may register. | `palw_state_v2.rs:7298` |
| O-12 | **`capable_classes` has two writers and every ADR-0071 amendment guards one** — a `BondRegistered` writes an unbounded, unchecked class set straight into `state_root`. | `palw_state_v2.rs:8151` |
| O-13 | **An attempt block's fork-choice blue work is a constant while its PoW is still priced by `header.bits`**, so the difficulty retarget bounds no header-DAG weight at all. | `ghostdag/protocol.rs:436` |
| O-14 | **The genesis free-prompt certified set names a dense-tier class the card does not register and omits the one it does**, killing the 489‰ tier's free-prompt lane from genesis. | `palw_e2e_adjudicability.rs:1048` |
| O-15 | **The deferred quality-bonus and reserve-drip fan-outs emit one coinbase output per included validator with no bound.** Same allowance as O-4's cap; every epoch-crossing block on a mainnet with a real validator set is invalid. (The cap itself is widened by this audit; the *unbounded* fan-out is not.) | `dns_finality.rs:3496` |
| O-16 | **`PruningPointPalwState.classCarriages` is uncapped**, each entry costing a full uncached state materialization, so one IBD peer can pin the single IBD latch forever. | `ibd/flow.rs:2554` |
| O-17 | **PALW material and receipt broadcasts are flood-relayed to every peer before any binding to a live claim**, at peer-count amplification, with no rate limit. | `v8/palw_gossip_flow.rs:52` |
| O-18 | **ADR-0084 D4's attempt-lane interval arm can never file `Valid`**: it compares the opening's real leaf count against `duty.work_leaves`, which every attempt claim sets to 0. | `palw_panel.rs:3979` |

## Open — should fix

`palw_state_v2.rs:8735` (a pending-chunk group whose part indices are not exactly `0..count` panics
the node on `BTreeMap` indexing, and the carriage loader never inspects `pending_chunks`) ·
`processor.rs:5467` (the object-acceptance rehearsal deep-clones the whole PALW state per carried
object, with the one attacker-sizable table inside the clone) · `palw_panel.rs:3512` (the opening
authorizer runs a full uncached materialization per request, gated by a per-peer counter and
reachable with a self-signed unbonded key) · `palw_mode_v2.rs:938` (the withdrawal-delay interlock
omits the DA-court windows, so arming `palw_da_court` lets a bond withdraw before its fraud stops
being provable) · `palw_e2e_adjudicability.rs:380` (the "family survives malformed material" half of
the covering set is a number the submitter writes) · `params.rs:4011` (on a carded mainnet a single
attempt block satisfies `required_work_depth`, collapsing the DNS overlay's PoW dimension).

## Unjudged — a finder substantiated it, no refuter reached it

The session limit killed these dimensions' verifiers. Listed so they are not mistaken for cleared.

* **free-prompt**: a claim's price is read from a job context the chain never sees and no seat
  compares to the job (`fp_interval.rs:1849`); the seat rebuilds every job context as if the run
  reached its budget, so a chain-legal `EndOfGeneration` claim is unjudgeable (`qwen25_a16_backend.rs:1483`);
  a claim's DA window is a number its own producer writes (`palw_state_v2.rs:9395`); ADR-0073 SA-1's
  beacon fold is dormant, so the producer that mines the beacon block gets a free re-roll (`processor.rs:7112`).
* **time/DAA**: the epoch-boundary chain block cannot be a non-floor attempt block — admission reads
  the parent's budget table against the child's epoch index (`palw_admission_v2.rs:357`); class
  reclamation folds idle streaks on gap epochs the sibling growth rule refuses to measure
  (`palw_state_v2.rs:7698`).
* **reward/mint**: PoS-v2 epoch tallies index blue-score attestation epochs with the DAA-denominated
  `epoch_length_blocks`, so `expected_stake` is evaluated at the wrong chain point (`dns_finality.rs:6886`).
* **gates**: the certificate that licenses a class to carry weight is granted by kernel-set
  containment alone, drilled on an n_ctx-32 toy geometry, so no drill vector can exercise a
  width-dependent court defect (`palw_e2e_adjudicability.rs:847`); ADR-0077 W10's interval-opening
  ceiling is an obligation stated only in a comment, with no registration-time check
  (`palw_gossip.rs:716`).
* **host security**: the panel's fee-funding recovery scan excludes exactly one bond outpoint rather
  than the chain's locked-collateral set, so a stranger can plant an unspendable "fee UTXO" at any
  operator's payout script (`palw_panel.rs:540`).

## Two claims the audit could not settle

Recorded as limits, not verdicts. The beacon and anchor walks are documented as reaching genesis but
use `default_backward_chain_iterator`, which terminates at ORIGIN — the pruning point on a pruned
node — and `derive_beacon_fact_to_genesis_v3` answers `prev_attempt_daa: 0` on exhaustion, which the
processor's own comment calls "inventing a witness" (`palw_fp_beacon_v3.rs:243`, `processor.rs:7089`,
`:6521`). Whether an archival and a pruned node can therefore derive different facts for the same
block was not established; it is the first thing to test next.
