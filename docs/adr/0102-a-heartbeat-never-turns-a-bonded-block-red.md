# ADR-0102 — A heartbeat never turns a bonded block red, and the clock steps aside for a draw that has landed

* Status: PROPOSED 2026-09-11 on `fix/heartbeat-trap-slow-producers` (from `main` at `a5f1bdf7`).
  **Decision 1 IMPLEMENTED behind `Params::palw_heartbeat_transparent`, `None` on every shipped
  preset** — no fingerprint, identity, schedule or fork id moves on any network, and arming it on
  testnet-11 is a flag day (§7). **Decision 2 IMPLEMENTED in the heartbeat miner** — node policy,
  no rule, safe to roll out host by host today. **Decision 3** is operator guidance, written into
  [`testnet11-node-operator.md`](../testnet11-node-operator.md) §7a.
* Builds on: [ADR-0060](0060-the-liveness-doctrine.md) Decisions 1–2 (the heartbeat lane and its
  ramp), [ADR-0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)
  Decisions 1–3 (the lane's own id and constant price; the one-block-deep slot rule; ε against the
  attempt lane's constant), [ADR-0068](0068-the-llm-primary-economy-and-the-floors-minimum.md)
  Phase 1 (`palw_attempt_work` = 2²⁰; the F3a width bound), [ADR-0058](0058-palw-merged-work-is-counted.md)
  (merged work — reds included — is applied to the PALW state), [ADR-0083](0083-the-difficulty-window-counts-only-rows-priced-by-bits.md).
* Amends: ADR-0066's "the ramp has two steps … the chain is producing, or it is not" — one sentence
  of it, the unstated premise that a producing chain produces faster than the recovery cadence.
  `heartbeat_interval_ms` and `check_heartbeat_slot` are unchanged.
* Supersedes nothing.

## 0. The sentence this ADR is

**From 11:41Z to ~13:20Z on 2026-09-10 testnet-11 ran on heartbeats alone, its DNS finality
stopped and its EVM bridge stayed paused, and nothing inside the chain could end it: a bonded block
that takes seventeen minutes to draw lands eight heartbeats behind the tip, and at `ghostdag_k = 1`
those eight heartbeats — one unit of work each — colored a block worth 2²⁰ of them RED.** Past this
ADR's fence a heartbeat never counts against a bonded block's coloring, so the heartbeat that merges
the draw carries its work and the DNS work depth passes the requirement at the first draw; and
today, fence or no fence, a heartbeat miner stands aside when a bonded block is waiting, so the next
draw takes the chain back — bounded, because a clock that waits on producers is the hostage
ADR-0060 was written to free.

## 1. What was measured (testnet-11, genesis `ad30b5cb…`, 2026-09-10)

* **Every block from 11:41Z was an algo-8 heartbeat**, two to four siblings every ~120 s. The
  fleet ran four heartbeat miners, all `--palw-heartbeat-miner-address`.
* **`getDnsConfirmation`**: `pow_confirmed = false`, `workDepth` ≈ 10 against `requiredWorkDepth`
  100, health `DegradedCertificateCensored`, the last DNS-confirmed anchor stuck. Attestations were
  being included — the stall was work, not stake. `dns_finality_fresh_for_bridge` requires
  `dns_confirmed`, so the bridge stayed paused.
* **Bonded blocks kept landing and kept losing.** Qwen3.6 draws on the fleet (8 cores, 23 GB,
  two 34 GiB mappings per host) take ~17 minutes, and a bonded block's timestamp is its TEMPLATE's.
  The draws that landed were merged RED — `59563aa3…` and `1b9225e1…` are two of them — so their
  2²⁰ entered no block's blue work. None became a chain block.
* **How it started**: node restarts at 11:18Z and 11:23Z lost the in-flight draws, the selected
  parent's timestamp plus the nominal hour passed, and a heartbeat became the selected parent.
* **How it ended**: the operator removed the heartbeat flag from all four miners. The next bonded
  block became the sink within minutes, and `workDepth` jumped to **1,048,584** — one attempt
  block's 2²⁰ plus eight ε. DNS confirmed at 13:26Z, a bridge claim was accepted at 13:27Z.

## 2. The mechanism

Five facts, each individually correct:

1. **The slot rule is one block deep** (ADR-0066 Decision 2): one nominal hour after a bonded
   selected parent, one recovery interval (120 s) after a heartbeat. It cannot see a bonded block
   that is not the selected parent.
2. **A bonded block's timestamp and parents are its template's.** The attempt envelope binds both
   (ADR-0042 Decision 3a), so a draw that took seventeen minutes lands carrying seventeen-minute-old
   parents.
3. **A block's blue work excludes its own work.** `mergeset_blues` holds the selected parent, not
   the block. A draw built on tip `T` has `blue_work = bw(T) + work(T)`; the heartbeat tip built in
   the meantime has `bw(T) + 8ε` or more. The draw is never selected.
4. **At `ghostdag_k = 1` a block with two blues in its anticone is red** (ADR-0058 said it: "any
   block whose anticone holds two or more blocks is a red *by construction*"). Eight heartbeats in
   the draw's anticone make it red, and a red's work is in nobody's blue work. ADR-0058 applies a
   red's PALW claim and pays it; it does not give it fork-choice weight.
5. **The DNS work depth is `blue_work(sink) − blue_work(anchor)`**, and the anchor advances with
   the attestation epochs. Inside the episode every chain step adds ε per blue heartbeat, so the
   work any current anchor can have piled on it stays near its distance in heartbeats — ~10 — and
   never reaches 100.

Together: **once a heartbeat is the selected parent, a producer slower than one recovery interval
can never become the selected parent again, and its work is lost while it tries.** The mode
sustains itself for as long as any heartbeat miner runs. `params.rs`'s premise that a draw takes
12–60 s under a 120 s block does not hold on the fleet, and the ramp has no step between "the chain
is producing" and "it is not".

## 3. The requirement

A heartbeat chain at the recovery cadence plus a bonded block submitted 17 minutes after its
template must eventually either become the selected parent or at least keep its work, so that the
DNS work depth keeps growing past `required_work_depth`. Without giving up what the lane is for:
keeping the DAA clock alive when every bonded lane is dead (ADR-0060 §2, §10) — so no rule may let a
producer, least of all a stuck one, stop the clock.

## 4. The candidates

| | breaks the trap? | keeps the draw's work? | one block deep? | total-collapse clock | cost / risk |
|---|---|---|---|---|---|
| (a) recovery cadence ≥ the preset's draw time | only while every draw beats it | only then | yes | slowed for good | the ambulance ADR-0060 §4 built |
| (b) hold the slot while a merged bonded block is recent | only if every heartbeat miner merges it | no — the first draw is red before any heartbeat can be held | yes (the width rule already reads mergeset headers) | **a stuck producer holds it** | fatal, below |
| (c) choose the selected parent by `blue_work + own work` | yes, at once | yes | yes | untouched | every ordering in consensus; circular fence |
| (d) a heartbeat does not count against a bonded block's coloring | keeps the work; the chain stays heartbeat-led | **yes, whatever the heartbeat miners do** | yes | untouched | one header read per peer past the fence; needs a depth bound |
| (e) operator guidance | no | no | — | — | needed anyway |

**(a) A longer recovery cadence** breaks the trap exactly while a draw lands before the next
heartbeat is allowed, so it would have to be tied to the slowest producer a preset will ever have,
and it fails the moment one is slower. It also takes back ADR-0060 §4's arithmetic: at a 25-minute
recovery cadence the 6,000-DAA lifecycle horizon a total collapse must sweep takes ~104 days instead
of ~8. It changes the one thing the lane exists to guarantee in order to accommodate producers the
lane exists to survive. Rejected.

**(b) Hold the slot for a merged bonded block** ("a heartbeat whose mergeset holds an attempt block
younger than the nominal hour is too early"). It is one block deep — the width rule already reads
every mergeset header — and it would end the mode when every miner cooperates: honest heartbeats
stop, the next draw lands on a tip nobody stacked ε on, and becomes the sink. Three reasons it is
not the rule:

* it cannot save the draw that STARTS the recovery: that draw was red in the anticone of heartbeats
  that existed before it did, and no rule about later heartbeats reaches them;
* alone, it is defeated by any heartbeat that does not merge the draw — a propagation race, or one
  miner that ignores bonded tips. The draw is red, so the next draw that merges it gains ε and not
  2²⁰, and the stray heartbeat chain ties or wins;
* **and it lets a producer whose blocks can never take the chain hold the clock.** Whether a bonded
  block may become the sink is chain state — a bond at its exposure ceiling produces blocks that are
  disqualified from the chain (the block-600 wedge ADR-0060 §1 lists first), and a key that pays the
  attempt lottery needs no bond to put a header-valid attempt block into the DAG. One block deep
  cannot tell those from a good draw. Under (b) each of them holds the lane for an hour (plus the
  future-time tolerance: 132 s on testnet-11, 1,620 s on a card), and a stream of them holds it for
  good — the wedge the clock was built to sweep, re-entering through the lane that sweeps it.

(b) survives as node POLICY with a budget, which is Decision 2.

**(c) Count a block's own work when choosing a selected parent** — Bitcoin's chainwork. It breaks the
trap at once: the draw's `bw(T) + ε + 2²⁰` beats the heartbeat tip's `bw(T) + 9ε`. It is also the most
invasive change available. Every ordering in consensus that uses blue work as "the heavier chain"
would have to move together or two of them would pick different chains inside one node:
`find_selected_parent` in GHOSTDAG; the virtual's sink search (`RankedTip`, `SortableBlock`);
`pick_virtual_parents`, which ASSERTS that the selected parent's blue work exceeds every other
candidate's; the pruning point and the pruning proof's level comparison; IBD's heavier-chain
checks; the DNS reorg gate's work dominance. The mergeset's topological order must stay blue-work
order, because the coloring loop relies on processing past-to-future. And it cannot be fenced the
way every other rule here is: it changes the selected parent, the selected parent determines the
mergeset, and the mergeset determines the DAA score a fence would be keyed on — so the fence would
have to be keyed on the parents' DAA scores and every site above re-derived under it. ADR-0058
already rejected its sibling ("weigh the candidate tip's own attempt in fork choice"). Rejected; the
trap does not need it.

**(d) Keep the merged draw blue against a heartbeat-only anticone.** The draw is merged — by the
next heartbeat or the next bonded block, whichever comes first — and under (d) it is merged BLUE, so
its 2²⁰ enters the merging block's blue work and every descendant's. That is the requirement's
second branch, and it holds **whatever the heartbeat miners do**: no cooperation, no timing, no
node-local fact. It is fenced the way ε already is: the rule is decided per merged block from that
block's own header (lane, DAA score), which is fixed before any block that merges it is colored.
What it does not do is make the draw a chain block — the heartbeat that merges it carries its work
and stays the selected parent, so the lane keeps running at the recovery cadence while miners run.
It needs one bound, derived in §5.1. **Chosen as the consensus rule.**

**(e) Operator guidance** — one heartbeat miner per operator, jitter, check the last bonded chain
block's timestamp before restarting a producer. All worth writing down (Decision 3), none a fix: one
miner still stacks eight ε during a seventeen-minute draw, and a restart is not the only way to lose
an hour of bonded production.

## 5. Decisions

### 5.1 Decision 1 (consensus, fenced) — a heartbeat never turns a bonded block red

Past `Params::palw_heartbeat_transparent`, GHOSTDAG colors each mergeset candidate by one of three
rules, chosen by the candidate's own header (`consensus/src/processes/ghostdag/protocol.rs`,
`LaneColoring`):

* **Classic** — below the fence, or on a network that has not armed it: the k-cluster rule over
  every blue, byte for byte what it was.
* **Weighted** — a non-heartbeat block past the fence. **Heartbeats are invisible to it**: a blue
  heartbeat in its anticone is not counted, the heartbeat's own count is not consulted, and neither
  enlarges the other's recorded count. Among non-heartbeat blocks it is the classic rule. The
  classic `|mergeset_blues| = k + 1` shortcut does not apply to it: that bound is a consequence of
  the k-cluster rule (every mergeset block is in the selected parent's anticone) and stops being one
  for a candidate that does not count a heartbeat selected parent — applied anyway, it is exactly the
  rule by which a heartbeat sibling that sorts first takes the only slot. Where the selected parent
  is itself non-heartbeat, its recorded count still caps weighted blues at `k + 1`.
* **Heartbeat** — a heartbeat past the fence: counted against every blue as before, shortcut
  included (it yields to bonded blocks, as it always did), but it never ENLARGES a non-heartbeat
  block's recorded count — that count is what a later bonded candidate is checked against, and a
  heartbeat must not be able to turn a bonded block red through a third block.

**The bound: the exemption stops at the merge-depth window.** A classic blue is inside the
merge-depth window by construction — a block further back has every chain block since in its
anticone — and `check_bounded_merge_depth` relies on that: it checks REDS against the merge-depth
root and never blues. A candidate that ignores heartbeats loses the automatic bound. Unbounded, a
bonded block withheld through a long heartbeat-only stretch would be colored blue arbitrarily deep —
deeper than any red the bounded-merge rule admits, outside the prunality argument's assumptions —
and its coloring walk would run back through every heartbeat to its parent, past a pruned node's
pruning point if it were old enough: a verdict that depends on what a node kept, the F4 shape
ADR-0066 deleted. So once a Weighted walk has passed over a heartbeat, it is red on reaching a chain
block whose blue score is below

    floor = blue_score(selected parent) + (blues already added + candidates not yet colored) − merge_depth

The parenthesis bounds the new block's final mergeset blues, so a candidate whose selected-chain
ancestor is at or above the floor is in the future of the new block's merge-depth root — exactly
where a red must be. At testnet-11's `merge_depth` of 30 blue score this is 13 recovery slots with
two blue heartbeats a slot (four miners give no more than two at `k = 1`), ~26 minutes, and ~56
minutes with one miner. A draw staler than that is red, as it is today, and — as today — cannot be
merged at all (`ViolatingBoundedMergeDepth`). The fleet's seventeen minutes is well inside.

**Keyed on the candidate's own DAA score**, the shape `palw_lane_blue_work_v1` already has: the key
is fixed before any block that merges the candidate is colored, so the rule never depends on the
new block's own GHOSTDAG output — which the new block's own DAA score does — and two builds agree
on every block below the height. The pruning proof's build and validate get the same fence through
`GhostdagManager::with_level`; heartbeats derive no block level, so above level 0 the rule has
nothing to apply to.

**The fence.** `Params::palw_heartbeat_transparent: Option<ForkActivation>`, a bare top-level fence:

| preset | value |
|---|---|
| testnet-11 (`palw_rc_shipped_params`) | `None` |
| devnet (`devnet_shipped_params`) | `None` |
| mainnet, the carded mainnet (`mainnet_card_base_v1`) | `None` |
| testnet-10, simnet | `None` |

Read through `Params::palw_heartbeat_transparent_fence()`, which folds in the mode and the heartbeat
lane (there is nothing for the rule to be about without heartbeats); `validate_palw_v2` refuses it
armed on a network whose lane is not. Wired the way every bare fence is: `palw_fences_v1` (so the
fork-id gate names a scheduled height — `fork_id_gate_fences_v1` is derived from it),
`for_each_fence` (Some-only, at the tail, so the identity normalises a scheduled height and
`fence_schedule_v1` carries it), `consensus_params_id` (Some-only, NAMED and WRITTEN — the step
ADR-0095's `palw_model_benefits` missed, `0448d955`), `consensus_schedule_id` (named, Some-only),
the `never() → None` collapse, and `override_params`. Every pin is unchanged, because a `None`
writes nothing.

### 5.2 Decision 2 (node policy, ships now) — the heartbeat miner steps aside for a landed draw

`palw_heartbeat_v1::heartbeat_yield_hint_v1` answers, from the virtual state (the selected
parent's lane; the lane and timestamp of every other block the virtual merges), one of three
things — three, because the miner has to act on each differently and one `Option` would fold two of
them into `None`:

* `BondedSelectedParent` — the chain is producing; a heartbeat-led episode, if there was one, is over;
* `NothingToYieldTo` — on heartbeats with no attempt block waiting: the regime the lane exists for;
* `YieldUntil(t)` — on heartbeats, and an attempt-lane block is waiting in the virtual's mergeset;
  `t` is the latest such block's `timestamp + HEARTBEAT_NOMINAL_INTERVAL_MS`, the hour the slot rule
  would have granted it had it been selected.

The miner (`kaspad/src/palw_heartbeat_miner.rs`) waits on `YieldUntil` once its slot is open, in
steps of at most a minute, charged to a **budget of one nominal hour per heartbeat-led episode**,
refilled only when a bonded block is the selected parent (observed on any pass, including one the
slot rule holds). Only attempt lanes count as something to yield to: they carry the weight and pay
the class lottery to exist; a receipt-lane block carries no weight a heartbeat could bury.

With the miners yielding, the producer's next draw lands on a tip nobody stacked ε on, becomes the
sink, and the slot rule's nominal hour holds the lane off again — **on the rule testnet-11 runs
today**. Without Decision 1 the first draw's work is still lost (it was red before anyone could
yield); with it, it is kept too.

The budget is what makes this safe to default on: a wedged bond, or a key that pays the lottery with
no bond, can put blocks into the mergeset that never take the chain, and each asks for its hour; the
episode pays for one hour in total and then the lane ticks at cadence until a bonded block really is
the selected parent. In a total collapse there is no attempt block and nothing to yield to. It is
advice: a heartbeat mined anyway — by a miner on an older build, or a stranger's — is exactly as valid
as before, and such a miner can keep the chain heartbeat-led (Decision 1 is what keeps that harmless).

### 5.3 Decision 3 (operators)

[`testnet11-node-operator.md`](../testnet11-node-operator.md) §7a: how to recognise the mode (every
block algo-8; `getDnsConfirmation` with `pow_confirmed = false` and `workDepth` below
`requiredWorkDepth`), how to escape it on a build without Decision 2 (stop every heartbeat miner for
one draw), and how not to enter it (check the last bonded chain block's TIMESTAMP before restarting
a producer; at most one heartbeat miner per operator, on the current build). The Relaunch-5 runbook's
"heartbeat miners on every host" is corrected there.

## 6. Safety and liveness, checked against the doctrine

* **The clock in a total collapse is byte-identical.** Decision 1 touches coloring only; the slot
  rule, the lane's price, ε and the width bound are unchanged. Decision 2 yields only to attempt
  blocks that exist.
* **A heartbeat flooder** loses a power it had: at `k = 1` two sibling heartbeats could make any
  bonded block beside them red; past the fence they cannot. Heartbeats still yield to bonded blocks
  and still meet the width bound (F3a), so flooding buys what it bought before — ε and fees — minus
  the ability to erase bonded work.
* **A bonded producer that backdates or forward-dates.** Decision 1 is keyed on DAA scores, which
  are derived from a block's parents rather than written by its producer, and ignores timestamps. Decision 2 reads the timestamp: backdating
  shortens the yield it earns; forward-dating lengthens it by at most the future-time tolerance, and
  the budget caps the episode either way.
* **A withheld bonded block** is blue only inside the merge-depth window — where a red would be
  admitted — and adds its weight to whichever chain merges it. A private chain gains nothing: its
  own bonded blocks are its chain blocks. The honest chain in a heartbeat-led episode is now as
  heavy as its bonded production, where before one private attempt block out-weighed the whole
  episode.
* **Sibling width (ADR-0066 F3a)** — unchanged, and no longer able to redden bonded blocks.
* **Blue score** can grow by more than `k + 1` per block past the fence: heartbeat blues and bonded
  blues no longer compete for the same `k + 1` slots, and under a heartbeat selected parent bonded
  candidates that are not in each other's anticone are all blue. Nothing in consensus assumes the
  bound (the coinbase cap is `mergeset_size_limit + 1 + extras`), and blue ⊆ mergeset, so blue
  score still grows no faster than the mergeset the DAA score counts.

## 7. What arming it takes

**testnet-11 (a flag day):** schedule `palw_heartbeat_transparent = Some(ForkActivation::new(H))` in
`palw_rc_base_params` with lead enough for every seat to rebuild, the way DAA 1150, 1900, 2150 and
2400 were scheduled. The printed fingerprint (`ecbdbc22…` in the release this ADR landed in) moves; the identity does not, so a build
without it peers until it is refused. The fork-id gate names H — and, as the third flag day measured,
an older build whose gate is already armed refuses the upgraded peer the moment it announces
`next = H`, so the notice must say *rebuild now, not by H*. Past H every node must carry the rule:
the blue set, blue work and blue score of every block whose mergeset holds a bonded block beside a
heartbeat anticone differ between the two rules. Pins to move deliberately: testnet-11's row in
`shipped_presets_have_pinned_fingerprints`; `the_shipped_schedules_are_measured_not_assumed` and
`the_gate_is_armed_only_where_a_shipped_preset_schedules_a_gate_fence` (the schedule gains H);
`fork_id_gate_fences_v1`'s invariance list in `the_fork_id_gate_names_every_scheduled_palw_fence`.

**A carded mainnet** can state it from genesis (`always()`), where it is free and the identity moves
by construction. Not done here: the card's armed set is pinned in
`a_carded_mainnet_arms_every_fence_testnet_11_arms` and mirrored in the README's activation table,
and changing what a card arms is a decision to take with that table open — ideally after a devnet
drill with the fence armed, which this change did not run.

## 8. Invariants the tests hold

* `a_slow_bonded_draw_is_red_behind_heartbeats_until_the_fence_keeps_its_work` — the incident in the
  pipeline (V2 at 120 s, heartbeat and attempt-work armed, two heartbeats a slot, draws landing 17
  minutes after their templates). **Fence dormant**: every draw red; each merging heartbeat adds
  **2** (two ε); no draw is ever a chain block; the chain stays at the recovery cadence; the work over
  the whole ~40-minute episode is **36 < 100**. **Fence armed**: every draw blue; each merging
  heartbeat adds **1,048,577** (2²⁰ + ε); the episode's work is **2,097,186**; and — stated — no draw
  is a chain block (Decision 1 keeps work, it does not hand the chain back).
* `the_fence_keeps_a_slow_draw_blue_only_inside_the_merge_depth_window` — blue exactly when the
  draw's chain ancestor is at or above the floor: 8 and 13 slots blue, 14 red, checked against the
  formula from stored blue scores; dormant, 8 slots red.
* `a_yielding_heartbeat_miner_hands_the_chain_back_to_the_next_draw` — both fence positions: the hint
  reads `BondedSelectedParent`, then `NothingToYieldTo` on heartbeats, then
  `YieldUntil(draw.timestamp + 1 h)` when the draw lands; with nobody mining, the next draw is the
  sink, the hint returns to `BondedSelectedParent`, and the next heartbeat's earliest slot is that
  draw's timestamp plus the nominal hour. The first draw is blue exactly when the fence is armed.
* Mutation check, run once while writing this: with `lane_coloring` forced to `Classic`, all three
  tests fail at their armed assertions.
* `palw_heartbeat_v1::the_yield_hint_separates_a_producing_chain_from_a_quiet_clock`; the miner's
  `yield_budget_tests` (a stream of blocks that never take the chain buys exactly one hour; only a
  bonded selected parent refills; minute steps; saturated deadlines).
* `the_heartbeat_transparent_fence_is_dormant_scheduled_or_a_rule_and_never_hides` — `None` on every
  preset; on testnet-11 a scheduled height keeps the identity and moves the fingerprint, the schedule
  id, the fence schedule and the fork-id gate; at genesis it moves the identity; refused without the
  lane.
* `fork_id_v1::every_scheduled_palw_fence_moves_the_fingerprint_and_the_schedule_and_never_the_identity`
  — for EVERY PALW fence, two future heights must print different fingerprints and schedule ids and
  one identity. Written listing `palw_model_benefits` as the one fence whose height the fingerprint
  did not carry (known since `0448d955`); the release this ADR landed in writes it (testnet-11's pin
  060e3597… → ecbdbc22…) and the list is empty, so the next fence forgotten the same way fails by name.

## 9. What is deliberately not decided, and what is not verified

* **Whether the chain should hand itself back without the miners' help.** Decision 1 keeps the work
  and Decision 2 ends the mode when the miners cooperate. A heartbeat miner that does not yield keeps
  the chain heartbeat-led — harmlessly past the fence, since every draw's work is counted. Making the
  bonded lane the selected chain again against an uncooperative miner needs chain state (whether a
  bonded block may become the sink), which is not one block deep; this ADR does not buy it.
* **Not verified on a network.** No multi-node drill has run with the fence armed; the pruning proof
  path with the fence armed is covered only by construction (the same `ghostdag()` with the same
  fence at both proof sites), not by a test that builds and validates a proof across it; and the
  yield has been exercised by unit tests and the pipeline hint, not by a running `kaspad` against a
  live chain.

## 10. Number hygiene and implementation record

`docs/adr/README.md` on `main` said the next free number was 0099; 0099–0101 are resident on other
branches (written 2026-09-10, not merged into this `main`), so this ADR takes **0102**, and the
README's hygiene row now says so. A concurrent claimant renumbers the later writer. **The next free
number is 0103.**

* **2026-09-11** — written and implemented on `fix/heartbeat-trap-slow-producers`:
  * `consensus/core/src/config/params.rs` — the fence, its accessor, the `validate_palw_v2` refusal,
    the identity/fingerprint/schedule wiring, and its test.
  * `consensus/core/src/fork_id_v1.rs` — the probe arm and the every-fence fingerprint test.
  * `consensus/core/src/palw_heartbeat_v1.rs` — `HeartbeatYieldHintV1`, `heartbeat_yield_hint_v1`.
  * `consensus/src/processes/ghostdag/protocol.rs` — `LaneColoring`, `HeartbeatTransparency`, the
    walk floor; `consensus/src/consensus/services.rs` and `processes/pruning_proof/{mod,build,validate}.rs`
    thread the fence to every GHOSTDAG manager.
  * `consensus/src/pipeline/virtual_processor/processor.rs` — `heartbeat_yield_hint`; the
    `ConsensusApi` method, its `Consensus` impl and the session proxy.
  * `kaspad/src/palw_heartbeat_miner.rs` — the yield and its budget.
  * `consensus/src/pipeline/virtual_processor/tests.rs` — the three pipeline tests above.
  * `docs/testnet11-node-operator.md` §7a; `docs/testnet11-relaunch5-runbook.md` (correction note);
    a pointer in ADR-0066.

  Verified on that tree with `MISAKA_PALW_POW_FIXTURE=1`: `cargo test -p kaspa-consensus-core --lib`
  1,973 passed / 0 failed / 10 ignored; `cargo test -p kaspa-consensus --lib` 295 / 0 / 7 (the three
  pipeline tests above and every existing heartbeat test among them); `cargo test -p kaspad --lib`
  85 / 0 / 0; `cargo test -p kaspa-testing-integration --lib -- ghostdag_test pruning` 3 / 0 / 3
  (`ghostdag_test`'s golden DAGs run the classic rule, which this change must leave byte-identical);
  `cargo clippy --tests` on the four touched crates adds no finding. `shipped_presets_have_pinned_fingerprints`
  passes unchanged: no pin moved.
