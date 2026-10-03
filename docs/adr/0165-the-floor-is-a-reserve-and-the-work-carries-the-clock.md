# ADR-0165 — Useful work carries the clock: the floor is the idle-only bonded fallback behind a floor state machine (A″), a REAL attempt carries the slot's tick (B); the bonded FALLBACK-V1 block and header-level floor invalidation are the next fence

**Status:** PROPOSED 2026-10-03 on `rcore/real-share` (lane RS), for the DAA-5,300 flag day (the Useful Work
Transition). **Revision 4** (coordinator decisions of 2026-10-03: FALLBACK-V1 is the NEXT fence, 5,300 ships A″ + B; after
P2's live measurement the floor became a rooted state machine with `floor_idle_slots` = 20 and `probe_cooldown_slots` = 20, and,
on P2's second replay, `probe_slots` = 8; header-level floor invalidation is **not** built: §10). Consensus change behind two
fences, **dormant on every shipped preset**: `Params::palw_floor_reserve_v1` (A″) and `Params::palw_real_clock_tick_v1` (B).

**Builds on:** ADR-0060/0064 (the liveness doctrine), ADR-0066 (the heartbeat lane), ADR-0105 (heartbeat
transparency, `LaneColoring`), ADR-0138 (the anchor clock), ADR-0142 (the cursor; §9 the floor), ADR-0045/0135
(classes, registry), ADR-0149 (an attempt's pwu is its weight), ADR-0152/0160 (the share and room gates, J-1's
per-bond weight cap).

## 00. The DAA-5,300 rule: A″ + B

### 00.1 A″ — the floor is the idle-only bonded fallback (`palw_floor_reserve_v1`)

Past the fence a `PALW-BASE-0` attempt — a *floor* attempt — is accepted by the fold **only while the chain's floor state
is Idle**, and is refused by name (`FloorNotIdle`) otherwise. The refusal is in the fold's class gate
(`check_class_admits_claim`), which the own attempt, a merged attempt and the producer's pre-check
(`palw_producer_facts_v2` → `class_admission_refusal` → `ready_to_produce_v3`) all ask. It comes before any write and
joins the pre-write skip arm, so the block stands and **no claim is written**: no reward is escrowed (the worker carve is
withheld and burned, as for every skipped own attempt) and no PALW weight is credited. **When the floor IS accepted
(Idle) it is today's bonded floor attempt, unchanged** — the same bond, producer-floor, exposure, share and room checks, the
same reservation, weight and reward. The rule adds a refusal and relaxes none; claims taken before the fence settle as before.
A **REAL attempt** is any attempt of a registered class other than the base class.

**The floor state** is one small rooted value, `PalwFloorStateV1 { mode, last_probe_end }`, with `mode` one of

* **Idle** — no REAL work is flowing; the bonded floor is the fallback and is accepted;
* **Probe { until }** — a REAL attempt was accepted *RED* after an idle stretch; the floor is refused through slot
  `until − 1`, so the next REAL attempt can land BLUE;
* **Normal { last_blue }** — REAL work is landing BLUE; the floor is refused while `daa − last_blue ≤ floor_idle_slots`;

and `last_probe_end` the DAA a probe last expired with no BLUE REAL attempt in it (`until`; `None` until one has) — the
cooldown's reference. The default (Idle, none) is stored as `None`, so a chain that never saw REAL work roots exactly as one
without the rule.

**One pure function moves it** — `palw_floor_step_v1(state, daa, accepted)`, `daa` the accepting block's DAA score (the slot),
*time first, then the event*:

```text
time   Normal ──(daa − last_blue > floor_idle_slots)──▶ Idle
       Probe  ──(daa ≥ until)──▶ Idle   [last_probe_end = until — the nominal end, never the DAA a block noticed it at]

event  a BLUE REAL attempt, in ANY mode ──▶ Normal { last_blue = daa }        (verified success: no cooldown)
       a RED REAL attempt in Normal or Probe ──▶ nothing                      (a RED one never extends)
       a RED REAL attempt in Idle ──▶ Probe { until = daa + probe_slots }     iff no probe has ended, or
                                                                              daa − last_probe_end ≥ probe_cooldown_slots;
                                                                              otherwise nothing
```

Floors are valid only in Idle. Time is idempotent and path independent (stepping to `t₁` then `t₂` is stepping to `t₂`), so
the result does not depend on block spacing. A BLUE attempt needs no cooldown; the cooldown exists only so that a stream of
RED ones cannot suppress the floor: a RED-only stream keeps it refused for at most `probe_slots` of every
`probe_slots + probe_cooldown_slots` (8 of 28), and never for a longer run than `probe_slots`
(`a_red_only_stream_cannot_keep_the_floor_refused_beyond_probe_slots_per_cooldown`).

**Events, their order and their colour.** An event is one REAL attempt the fold **fully accepted**: it passed every check the
fold makes — the registered, Active class, the bond (key, producer floor, freeze), the budget, the lottery, the room, the
exposure ceiling and share, the network room — **and wrote its claim**. The step runs inside `apply_attempt` just before
`write_claim`, so a merged attempt the fold later refuses is restored with its checkpoint, and an attempt the fold *skipped*
(refused by name, before any write) never moves the machine. **The kind a header claims never moves it; the claim does.**
Within a block the order is the fold's: the block's own attempt first (BLUE: a chain block), then the merged works in
consensus acceptance order, each stepping the state the one before left, all at the accepting block's DAA (Idle + [RED, BLUE]
is Probe then Normal; Idle + [BLUE, RED] is Normal and the RED changes nothing). A merged attempt's colour is its place in the
accepting block's mergeset: the processor hands the fold `GhostdagData::mergeset_reds` (`extras.merged_reds`), and the fold
reads a carrying block in it as RED, every other attempt as BLUE. The time step runs once at the start of every block's fold
(step 1e, before the sweeps and before any attempt, in the acceptance rehearsal too) so own and merged attempts read one state;
a floor attempt's gate reads the state at its place in the order.

**Rooted and reorg-safe.** The field `floor_state` is in the state root (only when `Some`), the pruning carriage (tail `0xEA`,
only when `Some`) and the delta (variant 104, `RealWork { old, new }`; **one entry per change, exact revert and apply**, so a
reorg restores it and a branch computes its own); time writes once (`time_writes_the_floor_state_once`). A node that joins by
pruned sync gets it in the carriage, checked against the root the witness header commits
(`real_share_a_pruned_join_inside_a_normal_stretch_…`, `…_inside_a_probe_…`: 00.8).

### 00.2 The live finding, what A″ answers — and what it does not

**Evidence** (P2, `lanes/evidence/8k-red-1003/`, the last 300 DAA of testnet-12 before 2026-10-03): 72 attempts of the 8k
class, **71 RED and 1 BLUE; every RED caused by floor attempts** (141 counted peers, all floors; heartbeats counted 0). The 8k
producer infers for 342 s at the median (p95 418 s) while floors — 3.0 a slot, 64 % of them external miners — extend the chain.

**Mechanism.** ADR-0105's `LaneColoring::Weighted` makes heartbeats invisible to a bonded candidate; a floor attempt is
another attempt and is counted classically. Testnet-12's `ghostdag_k` is 1, so two floor-lane blocks in an attempt's anticone
make it RED, and 340 s holds about eight. The colouring is header-stage: it reads the lane, not the class and not the state.

**What A″ does.** While REAL work lands BLUE (Normal) or has just landed RED (Probe) the floor is refused by the fold **and by
the producer's pre-check**, so honest floor producers hold before they spend a draw; only heartbeats extend the chain under a
slow attempt, and it is BLUE. `t12_real_share` runs this on testnet-12's own GHOSTDAG through the real pipeline: before the
fence a slow REAL attempt with two or three floors in its anticone is RED (the finding reproduced) and with the heartbeats of
one, two or three slots it is the selected parent; past the fence, with the floor refused, it is BLUE, accepted and refreshes
Normal; after Normal has run out the first REAL attempt can go RED, opens a Probe, and the next one is BLUE.

**What it does not do — the honest limit, with numbers.** **A″ is a rule about which blocks earn a claim; it is not a rule
about which blocks exist.** A floor-lane block the fold refuses is still a valid block in the DAG, and GHOSTDAG still counts it
against a slow REAL attempt: the header-stage colouring cannot read the class or the state. What protects the REAL attempts is
that **honest floor producers do not mine while the state says Probe or Normal** — the producer's default hold, which asks the
fold's own gate — and that a producer that ignores the hold earns nothing for it (its blocks carry no claim, no weight, no
reward). P2's second replay of the live capture (`lanes/evidence/8k-red-1003/replay2/`, 72 REAL attempts, DAA 3,779–4,219; *REAL
BLUE* = the share of REAL attempts GHOSTDAG colours BLUE) measures how much rests on that compliance:

| who ignores the hold | REAL BLUE at 20 / 8 / 20 (shipped) | at probe 6 |
|---|---|---|
| live, no rule at all | 1.4 % (1 of 72) | 1.4 % |
| **everybody — the fold's refusal alone, nobody holds** | **1.4 % (1 of 72)** | 1.4 % |
| **nobody — every floor producer holds** | **97.2 % (70 of 72)**; REAL attempts are 93.3 % of the BLUE attempts GHOSTDAG counts | 88.9 % (64 of 72) |
| one external producer, 9.5 % of the floors | 72.2 % (52 of 72) | 68.1 % (49 of 72) |
| the largest external producer, 38 % of the floors | 29.2 % | 19.4 % |
| the fleet's own producers, 36 % of the floors | 18.1 % | 13.9 % |
| the small externals together, 26 % of the floors | 38.9 % | 20.8 % |
| every external producer, 64 % of the floors | 6.9 % | 5.6 % |

(The first three rows, the fleet row and the last are P2's `final2.json` sweep; the per-producer rows are P2's `perprod.py`, rerun
here with the shipped probe as well.) **The fold-level refusal without the producer hold is worthless** (1.4 % BLUE — the live figure), and
**one non-compliant floor producer at a tenth of the floors takes REAL BLUE from 97 % to 72 %.** So the hold is **ON by default in
every role that mines floors** — the kaspad PALW producer service, the only one — and has no off switch except the drill flag
`--palw-drill-floor-ignore-policy` (a salted private drill only). Non-compliance is not version skew: a binary without the rule
forks off at the flag day, so a non-compliant floor producer on the chain is a *modified* one, whose only motive is to degrade
REAL work (its floors earn nothing). The cure that does not depend on anyone's compliance is the next fence (§10); this one is a
policy the producers must follow, and says so.

### 00.3 The constants, from the measurements

All three are named constants in `palw_real_share_v1`, **hashed with the fence** (`palw_floor_reserve_value_v1` →
`[20, 8, 20]`: changing one is a new network id), each checked against its derivation.

* **`floor_idle_slots` = 20** (40 minutes) — `⌈p95 gap between accepted REAL attempts / slot⌉ + margin` = `⌈17.3⌉ + 2`
  (`palw_floor_idle_slots_for_v1`; P2's gap: p50 3.1 slots, p95 17.3, max 30.6 — the long gaps are panel and backpressure
  holds; the margin is one slot of accepting-block lag and one of rounding and jitter; the inference is *inside* the gap so it
  is not added), capped at 30 (one hour, the merge-depth duration). The sweep (full compliance, probe 8) is why 20 and not
  less: REAL attempts are **47 % of the BLUE attempts GHOSTDAG counts at 12, 69 % at 16, 93 % at 20**, and 100 % at 24 and 30
  (the rest are floors accepted in Idle); more than 20 only lengthens the unbonded stretch after REAL work stops (00.4).
* **`probe_slots` = 8** (16 minutes) — `⌈median draw / slot⌉ + ⌈longest draw / slot⌉ + lag` = `3 + 4 + 1`
  (`palw_floor_probe_slots_for_v1`; P2's second replay: the 8k producer's draws take 325 s at the median and 426 s at the
  longest). A Probe opens when a REAL attempt lands RED (the trigger). The producer's next attempt was templated before it, so
  it collides with the probe's start; the one after it is the first templated under the probe, with no floor under it, and the
  probe must outlast its whole draw. **Full compliance, REAL BLUE: probe 4 or 6 gives 63–89 % (idle window 12–30), probe 8 or
  more gives 93–99 %** — which is why it is 8 and not the 6 of revision 3.
* **`probe_cooldown_slots` = 20** — the least gap from the end of one unanswered probe to the start of the next. At full
  compliance cooldowns of 10, 20 and 40 measured 91.7 %, 88.9 % and 90.3 % REAL BLUE (probe 6): it is not what the result turns on;
  it bounds a RED-only stream to 8 refused slots of every 28.

**What the constants buy, as measured at 20 / 8 / 20 with full compliance:** REAL BLUE 97.2 %, and the chain is Idle — the floor
is the fallback — for 1.2 % of the wall time. The **empty-slot share** (slots with no accepted attempt of either kind, the
heartbeat carrying the clock alone) is 81 % at these constants: the 8k stream is about one attempt in three slots, so the chain is
mostly heartbeats between attempts. A″ makes the attempts that exist land; it does not make more of them.

### 00.4 The price

* **After REAL work stops, up to `floor_idle_slots` = 20 slots (40 minutes) are heartbeat-only** — unbonded, weight ε a block —
  before the bonded floor resumes. A transaction confirmed only inside that stretch is as exposed as inside any heartbeat-only
  stretch today (`hb_probe_b_*`, `hb_probe_d_only_the_finality_depth_stops_a_heartbeat_only_reorg`): a rewriter needs more beats a
  slot than the honest chain and no bond, until the finality depth or a Settlement Anchor stops it. The stretch is bounded by the
  constant (and the constant by its cap), not open-ended. A Probe adds at most 8 refused slots of floor per 28 to a RED-only stream.
* **While REAL work flows, the chain is better secured than by floors, not worse.** An 8k claim weighs 1,388,926,972,416 pwu
  against a floor's 6,042,506,112 (229.8×). Three floors a slot is 18.1e9 a slot; the 8k stream at its median gap is 448e9 a slot
  (24.7×) and even at its p95 gap 80e9 a slot (4.4×).
* **Weight concentrates in the REAL producers' bonds** while it flows, because the floors that used to add to it are refused.
  That is the intent (the floor is a fallback), and it is the cost: the rewrite cost of a busy stretch is the REAL claims' weight,
  bonded and panel-verified, which a rewriter must out-weigh with REAL work of its own.
* **Normal is kept alive by BLUE REAL attempts only.** A producer holding a bond and any Active REAL class can keep the floor
  refused indefinitely by landing one BLUE REAL attempt every `floor_idle_slots` slots; the chain's weight then grows by at least
  that attempt's weight and the stretch is *not* heartbeat-only (each REAL attempt is bonded and panel-verified), but a cheap
  class could make that weight small. A RED stream cannot do it (cooldown). **Open, recommended follow-up (a decision, not done
  here):** step the machine only for a REAL attempt whose pwu is at least a stated multiple of the floor's, so a token class
  cannot suppress the floor. Without it the guard is the class's registration burn, its panel verification and the bond's capacity.
* **Withholding REAL work to force the fallback gains nothing:** a REAL producer that withholds goes idle after
  `floor_idle_slots`, from where the floor — bonded weight, reward as today — is what it could have earned anyway, and it
  forgoes the REAL attempts' weight and reward.
* **A modified floor producer that ignores the hold forgoes everything and can still hurt** (00.2): its blocks are valid and
  colour classically. That is the limit of a fold-level rule and the reason for §10.

### 00.5 Reds count — and never extend

`palw_v2_merged_works` returns the work of the whole mergeset — blues and reds alike, the selected parent excluded (its own
work is applied by its own fold); its comment: "at `ghostdag_k = 1`, any block whose anticone holds two or more blocks is a red by
construction … A blues-only rule would measure nothing". A RED REAL attempt is therefore applied by the merging block's fold,
exactly as a BLUE one — its claim, its weight, its reward — and it is an event of the machine with colour RED: **in Idle it opens
a Probe** (once per cooldown), in Normal and Probe it changes nothing. Tested through the pipeline
(`…a_rogue_floor_still_colours_classically_but_earns_nothing_and_a_red_real_attempt_never_extends_normal`: the attempt is RED, its
claim exists, its weight is credited, Normal still ends `floor_idle_slots` after the BLUE attempt, not after it;
`…a_red_first_attempt_opens_a_probe_…`). A non-DAA block (outside the window or a round block) is the one exclusion, as it is for the
coinbase.

### 00.6 B — a REAL attempt carries the slot's tick (`palw_real_clock_tick_v1`)

**What cannot be done, and what is done instead.** The DAA score is derived at **header** processing from the window and the
mergeset's HEADERS (ADR-0142 §6a: no stored cursor, so a pruned and an archival node agree). A block's class and bond live in its
body and are judged at the virtual stage. A tick conditioned on "an accepted REAL attempt" would make a header-stage value depend
on virtual state — circular, and divergent between a node holding the bodies and one that does not. So the consensus tick source is
the **lane**, which a header states: an attempt-lane block (`is_palw_attempt_algo_id`: algos 6 and 9), beside the heartbeat lane.
The preference for REAL work is the product of A″ (no honest producer mines a floor while a REAL attempt is recent) and of the
miner's policy, not of a clock rule.

**The rule.** In `palw_clock_step_v1`, past the fence: a mergeset with nothing `bits` prices and at least one tick source (a
heartbeat or an attempt-lane block) is `granted` when the NEWEST source is stamped at or past the cursor's slot. `granted` removes
exactly ONE exemption (`palw_clock_tick_source_v1`), so a mergeset advances the DAA by one however many attempts it holds — *never
each attempt +1*. The cursor, the reference (earliest tied step), H3, H5 and the lead cap are unchanged; the lead cap now also
covers an attempt-lane header. **Node policy**: `heartbeat_slot_hint_v1` — the heartbeat miner holds until `slot + 20 s`
(`PALW_REAL_TICK_GRACE_MS_V1`) for an attempt to carry the tick, and then mints as before.

| property | holds because |
|---|---|
| a producer cannot **speed up** the clock | a tick needs a source stamped ≥ the cursor's slot and the step that consumes it is stamped ≥ that slot (H5) and ≤ receiver clock + 132 s (lead cap): references are ≥ one interval apart. Attempts are cheap at the header stage and a heartbeat costs 2^24, but cost never bounded the RATE — the cursor does. A burst is `⌊132/120⌋ + 1 = 2` ticks, once. `a_producer_of_any_mix_cannot_run_the_clock_faster_than_a_heartbeat_miner` simulates adversarial mixes of up to 100 attempts and 3 beats per instant. |
| cannot **stall** it beyond a heartbeat miner | the reference is the earliest tied step; any honest block merging a granted source is a step. |
| one tick per slot however many attempts | `granted` removes one exemption: `a_hundred_attempts_in_one_slot_advance_the_score_by_one`; through the pipeline, `…a_real_attempt_carries_its_slots_tick_and_a_slot_ticks_once` (an attempts-only chain: one tick a slot with no heartbeat anywhere; the unarmed control ticks not at all). |
| DAA-denominated windows keep wall meaning | the DAA still moves at most once per 120 s. |
| reorg / pruned node | `attempt_ticks`, `granted` and the cursor are functions of the block's own parents and window; no new state; headers only. |

### 00.7 Node and RPC

* `getBlock` verboseData carries **`blockKind`** (JSON only; the borsh wire and the gRPC proto are unchanged): `REAL` (a non-floor
  attempt), `FALLBACK` (a floor attempt past the fence), `LEGACY_FLOOR` (before it), `LEGACY_HEARTBEAT` (algo 8), `EXEC` (a round
  block), empty otherwise (`palw_block_kind_v1`, tested against a real `palw_commitment`).
* **`[palw-floor-state]`** — one info line per chain block whose fold moved the floor state, naming the net move:
  `[palw-floor-state] daa=<N> block=<hash> <from>-><to> last_blue=<n|-> until=<n|-> last_probe_end=<n|->`, `from`/`to` one of
  `idle`, `probe`, `normal`, the three fields describing the new state (`palw_floor_state_log_line_v1`, format pinned by a test).
* **The producer's hold** (the default policy, nothing to enable): while the state at the candidate's DAA is not Idle the floor
  producer logs `[palw-producer] holding: the model registry admits no new claim of this class now [class=<id> epoch=<n> …
  registry="the base class <id> is the idle-only fallback: at DAA <n> the chain is <state> (ADR-0165: floors are refused until it
  is idle)"]`, at **info** and never as a starvation error (`palw_hold_level_v1`: it is the design working). The grep marker is
  `idle-only fallback` (`PALW_FLOOR_NOT_IDLE_MARKER_V1`, pinned to the error's message by a test). It resumes by itself when the
  state is Idle. The fold's own skip of a refused floor is logged by the processor as `PALW: merged blue <hash> carried work this
  chain point refused (the accepting block stands): the base class … is the idle-only fallback …`.
* **Anchor duty** (§00.9): `[palw-producer] anchor duty: the floor is held as the idle-only fallback, and a claim has waited <n> slots (since
  DAA <d>) for an operator attempt — mining ONE binder, refused by the fold (no claim, no weight) and still an operator attempt that binds
  the due claims`, at info. Only for an operator's floor producer (`binder_due` is false for any other bond).
* **The lane watch** (`palw_lane_watch`) counts an attempt-lane block a selected-chain block MERGED, blue or red, as PALW work: under A″ the
  REAL attempts land beside the chain (none of the 86 in P2's capture is a chain block), and a watch that read the selected chain alone
  would call a healthy chain "running on its clock alone" and raise its ERROR continuously. A chain of heartbeats with nothing merged
  still alarms.
* **Drill flags** (salted testnet-12 chains only; each refused without the salt at start-up and re-checked against the chain the
  node runs): `--palw-drill-useful-work-at=H` arms both fences at a low height; **`--palw-drill-real-submit-delay-s[=N]`** holds a
  REAL (non-floor) attempt's submission until N seconds after its template (default 340 s, the 8k median; 1..=3,600) to emulate an
  8k inference's minutes on a drill-sized class; **`--palw-drill-floor-ignore-policy`** makes the floor producer ignore *only* the
  idle-only refusal — the drill's policy-ignoring floor miner (00.2), a switch that takes no value.

### 00.8 Tests

`consensus/core` — `palw_real_share_v1` (every arc of the machine, time idempotent and path independent, ordering inside a slot,
the RED-only bound, BLUE sustains Normal and Normal ends `floor_idle_slots` after the last, BLUE after an expired probe is Normal,
the constants and their derivations and caps, the canonical rooted form, the log line and the net move, the refusal's words, the
tick-source rule, the clock-speed simulation), `real_work_reserve_v1` (the fold around the machine: Idle accepts, Probe and Normal
refuse by name, an unanswered probe expires and the cooldown holds RED off but not BLUE, RED never extends and BLUE does, several
attempts in one mergeset in the fold's order, **only a fully accepted REAL attempt moves it**, fence straddle, reorg revert/apply and
branches, the carriage, time writes once, an all-floor network is Idle and live), `palw_useful_work_fences` (dormant on every preset,
fingerprint and fork id, prerequisites). Pipeline, on testnet-12's own GHOSTDAG — `t12_real_share` (00.2, 00.5, 00.6: the slow
attempt under floors and under heartbeats, the busy chain, the rogue floor, the RED first attempt and its Probe, an unanswered probe
expiring, B), **the pruned join** (`…a_pruned_join_inside_a_normal_stretch_…`, `…_inside_a_probe_…`: a second node follows the
archival one through the pruning point, is left as a pruned join leaves it, imports the carriage against the witness child's committed
root and then agrees with the archival node on every block — sink, root, floor state and the decision on each floor attempt), and
`t12_post_launch_fences_combined` (the two fences armed with the whole release at one height: the floor reserve is live, an
all-floor chain stays Idle and roots as before, the clock still runs). `kaspad` — the drill flags (refusals, the wait rule, the
policy bypass and its wiring), the hold's wording and level, the anchor duty's rule, the lane watch counting merged attempts. `rpc-service` — `blockKind`.
The anchor finding (00.9) is measured by `real_share_gap_*` and `real_share_control_*` in `t12_real_share`.

### 00.9 What A″ does to anchors (open finding of 2026-10-03; P2, `lanes/evidence/head-admission-slip-1003/`)

**The floor was the chain's anchor factory.** In P2's capture every attempt on the selected chain is a floor (1,415 of 1,415 chain
attempt blocks); **none of the 86 REAL attempts is a chain block** — an 8k attempt is templated slots before it lands, the heartbeat
chain that grew meanwhile (about 2^24 of work a beat) out-weighs it, and the next block merges it beside the chain — and **all 86 are
the fleet's own genesis bond**, an *operator* by lane A's reckoning. Two consumers read the admitted attempts of chain blocks, and A″
holds the producers that made them:

1. **ADR-0130's round seed anchor.** Recorded by a chain block whose OWN attempt is admitted (`record_round_seed_anchor`, step 4's
   `Ok` arm), read by the next span's schedule and by the ADR-0147 admission jury (`round_seed_anchor.span + 1 == span_now`, else no
   audit and the class stays `Candidate`; no schedule and the execution lane is idle for the span). With the floor held a span has an
   anchor only if a REAL attempt was a chain block in it — which none is.
2. **Lane A's panel binding.** A claim binds only in a block that is or merges an OPERATOR's attempt at or past its slot
   (`palw_chain_block_as_anchor_v1`). Today the REAL attempts are the fleet's, so they anchor; where REAL work comes from
   non-operators and the operators' floor producers hold, **no operator attempt exists and the claim waits for its backstop**
   (`bind_base + window_bind`, 580 DAA, then `BindTimeout` without forfeit).

**Measured in-tree** (testnet-12's own pipeline; A″ + B armed; lane A and lane F1 armed with 4 of the 8 genesis cards as operators;
spans are one DAA here; `t12_real_share::real_share_gap_*` and `…_control_*`, which assert the gap as it stands — flip them when a fix
lands):

| | result |
|---|---|
| a non-operator's REAL claim, chain Normal, floor producers holding, non-operator REAL attempts keeping it Normal | slot DAA 30; at DAA 45 still **`Provisional`** |
| the same, then ONE operator floor — refused by the fold (`FloorNotIdle`: no claim, no weight) | **`PanelBound { bound_daa: 45 }`** at once: the anchor needs the operator's *attempt* (its header), not its claim |
| the same claim with the REAL attempts made by operators | `PanelBound { bound_daa: 41 }` |
| seed anchors, the same run | **1 of 36 spans anchored** (the opening REAL attempt, a chain block); the REAL attempts merged beside the chain anchor nothing |
| seed anchors, a denser run (one REAL attempt in flight at a time, four merged beside the chain, none the selected parent of its merger) against the floor producers mining every slot | Normal, floor held: **1 of 16 spans anchored**; honest floors, Idle: **15 of 16** |

**What was decided** (coordinator, 2026-10-03; none of it changes the state machine):

* **Anchor duty — built, in this fence's release, default on** (producer only, no consensus change). An operator's floor producer that
  holds *only* because of the idle-only policy mines ONE binder when a claim has waited 30 slots for an operator attempt (staggered 0–7
  slots by bond so the operators do not all fire in one slot, and the wait starts again after it fires:
  `palw_floor_anchor_duty_v1`). A refused floor is still an operator attempt, so it binds the due claims (measured above); the cost is
  about one floor per 30 slots in the non-operator-REAL regime and none while operators' REAL attempts (a 3-slot gap) anchor by themselves.
  It is the only floor the hold lets through, it replaces the time-triggered "beacon floor" an earlier proposal gave the replay, and it is
  logged (§00.7).
* **The round seed anchor — NOT in this fence.** Merged admitted attempts recording the anchor, the anchor kept across spans, and the jury
  and the schedule seeding reading the latest anchor within the last 24 spans are the separate 5,300 fence `palw_anchor_window_v1`
  (lane P2, branch `anchor/window`, user-approved). This ADR neither implements nor assumes it: without it, A″ leaves the ADR-0130 schedule
  and the ADR-0147 admission jury without an anchor in almost every span (measured above), and the tests that assert that gap are to be
  flipped when it merges.
* **Class-aware colouring — the next fence** (§10.3), handed to lane RF8 (RFC-0008) as the alternative to its merge-admission design.

## 0. Where the implementation stands

| piece | state |
|---|---|
| **B** — a REAL attempt carries the slot's tick, one tick per slot (`palw_real_clock_tick_v1`) | built, tested (00.6) |
| **A″** — the floor is the idle-only bonded fallback, behind the floor state machine, 20 / 8 / 20 (`palw_floor_reserve_v1`) | built, tested (00.1–00.5); **the producer hold is default-on** |
| **FALLBACK-V1** — the bonded fallback block, its weight, reward, cap, idle rule, retiring the heartbeat | **designed here (§5–§8), the next fence, not built** |
| anchors under Normal (lane A's binding; the round seed and the jury) | **00.9**: measured in-tree; **anchor duty built** (binding); the round seed / jury are `palw_anchor_window_v1`'s (separate fence); class-aware colouring (§10.3) is the next fence |
| header-level floor invalidation / class-aware colouring / re-anchored inference | **open problems, §10, not built** |

## 1. The defect

The 600 blocks of testnet-12 at DAA 3,939–4,071: 67 % attempts, 33 % heartbeats, and almost every attempt the
`PALW-BASE-0` floor class. Two block kinds exist to keep the chain alive — the floor attempt and the heartbeat — and
together they were two thirds of the chain. Neither is a model. And the floor was not only idle weight: it buried the
models that did run (00.2).

## 2. The block kinds

| kind | what it is | claim / panel / court | clock | weight | reward |
|---|---|---|---|---|---|
| **REAL** | an attempt of a registered, Active class other than `PALW-BASE-0` | yes (unchanged) | carries the slot's tick (B) | its pwu (ADR-0149) | unchanged |
| **EXEC** | the execution lane's round block (ADR-0125) | permit | none (outside the DAA set) | unchanged | unchanged |
| **FALLBACK** (5,300) | a floor attempt past the fence, accepted only while Idle (A″) | yes — it IS the floor claim | carries the tick like any attempt | today's floor weight | today's |
| **FALLBACK-V1** (next fence) | a bonded, idle-only block that is not a model class (§5) | **none** | carries the slot's tick when nothing REAL did | bond-derived `w_fb`, one per bond per slot | fee-only |
| **LEGACY_HEARTBEAT / LEGACY_FLOOR** | the pre-fence heartbeat / floor attempt | as before | as before | as before | as before |

## 3. How the rule got here (and A′, superseded)

Revision 1's hysteresis ledger (dormant at ≥ 12 Final real claims in 60 DAA) was withdrawn for a single number, the DAA of the last
accepted REAL attempt. Revision 2's A′ retired the floor outright — not safe alone: with few REAL producers a stretch would be
heartbeat-only (unbonded, ~0 weight, cheap to rewrite). Revision 3 kept the floor as the fallback behind that single number with
K = 20. Its two flaws, found on P2's replay: it could not tell a BLUE attempt from a RED one, so a RED attempt (which verifies
nothing about the chain being free of floors) held the floor off as long as a BLUE one; and it had no answer for the first REAL
attempt after an idle stretch, which met the resumed floors and went RED. Revision 4's machine fixes both: BLUE is verified success
and holds the floor off (Normal); a RED attempt in Idle opens a bounded Probe for the next one to land clean and never extends
anything; and a RED-only stream is bounded by the cooldown.

## 4. B — see 00.6

## 5. FALLBACK-V1 (the next fence; design, not built)

A **block kind**, not a model class: no claim, no panel, no court, no TIR. It is the heartbeat lane (algo 8, its fixed 2^24
canonical hash puzzle, ε `blue_work`, declared subsidy 0, the stamp/slot/lead-cap rules of ADR-0142) with three additions and
one retirement:

1. **Bond-required.** The block carries a fallback envelope `{ version, operator bond outpoint, pubkey, ML-DSA-87 signature
   over (domain "MISAKA-FALLBACK-V1", selected parent, DAA score, bond) }`, verified statelessly against the embedded pubkey
   (a malformed or unsigned algo-8 block is invalid past the fence — **the heartbeat's retirement**). The fold checks the bond:
   registered, the pubkey is the bond's, not `Retiring`, not frozen, `palw_bond_may_take_work_v2` — *exactly the attempt
   producer's eligibility*. It reuses the attempt signature context; a **new** context is not an option, because
   `signature_contexts_root` is part of testnet-12's ruleset id and adding one would re-mint the network.
2. **Idle-only.** Valid for weight and reward only when the floor state of 00.1 is Idle, read from the block's own parent state.
   A fallback that fails any fold check is *skipped*; its header-level tick is unaffected (00.6: lane-based).
3. **Capped.** One credited fallback block per bond per slot (J-1's per-bond cap restated), in a rooted `bond → last credited
   DAA` map.

## 6. Weight, reward and the order of kinds (FALLBACK-V1)

* **Credit.** A valid fallback adds `w_fb` to a rooted `fallback_weight`, and the economic fork-choice key becomes `(safe_frontier,
  safe_weight + fallback_weight, immature)` — a chain whose frontier advances beats a fallback-only chain on the first key.
  `w_fb` is one floor claim's canonical weight (604,250,611, ADR-0160 Appendix A).
* **REAL > FALLBACK by construction.** A REAL attempt's credited weight is `max(its pwu-derived weight, 2·w_fb)`.
* **Reward.** Fee-only; REAL pays its subsidy share. **Useful-work accounting: 0.**

## 7. Security analysis (FALLBACK-V1)

* **A fallback-only stretch is as expensive to rewrite as today's floor stretch**: it weighs `S·B·w_fb` for `B` bonds credited a
  slot, each eligible exactly as a producer and credited once a slot — the same collateral and the same time as the honest
  stretch, and not cheaper by any hash trick: the puzzle is the heartbeat's fixed 2^24, which secures nothing by itself
  (t12's PoW target is 1); the security was always the bonds.
* **Withholding REAL work to force the fallback gains nothing:** it gives up REAL's weight and reward for strictly less of both.
* **Free tick sources** can move the DAA at most once per slot (00.6); bond, idleness and cap are fold rules about WEIGHT.
* **Reorg determinism.** Idle, bond eligibility, the per-bond slot and the credit are functions of the parent state and the block;
  each write is a delta entry reverted exactly.

## 8. What FALLBACK-V1 costs

Three rooted fields (the floor state exists; `fallback_weight` and the per-bond map are new: root, carriage tails, delta
variants), a stateless envelope check on algo-8 headers past the fence (`palw_commitment` is hashed only for PALW algos, so either
the envelope rides in the coinbase payload or the hashing gate widens behind the fence), the fold step and its refusals, the
fork-choice key change, the `max(…, 2·w_fb)` weight floor, a producer and miner change, tests and a drill; about 20 hours and two
re-pins. It is the next fence by decision; 5,300 is a sound release without it (00.1–00.6).

## 9. Prerequisites and the release

`validate_palw_useful_work_v1`: A″ needs ConsensusV2 and `palw_model_registry` at or below it, and the bundle's mirror
(`sync_palw_floor_reserve_v1`); B needs ConsensusV2 and `palw_anchor_clock`, `palw_single_lottery`, `palw_clock_cursor`,
`palw_clock_floor` and `palw_clock_lead_cap` at or below it. Release entries: `PALW_T12_FLOOR_RESERVE_ENTRY`,
`PALW_T12_REAL_CLOCK_TICK_ENTRY` (`PALW_T12_USEFUL_WORK_FENCES_V1`); the fence name `palw_floor_reserve_v1` is kept for A″.
State: delta variant 104, carriage tail `0xEA`. The three constants are in the fingerprint (`palw_floor_reserve_value_v1`).

## 10. The next fence: closing the policy gap (open; none of this is built)

The fold-level rule leaves one thing open, and 00.2 measured how much it matters: **a floor-lane block the fold refuses still
exists, and still colours.** Three directions, an honest account of each, and the boundary problem that makes all of them hard.

**10.1 The boundary problem.** GHOSTDAG colouring and the DAA are **header-stage**: a node orders headers first (IBD,
pruning-proof sync) and judges bodies and state later at the virtual stage; the class, the bond, the room and the registry live in
state. Anything that must stop a floor block from counting *at colouring* has to be decided from header data alone. A header names
its lane and its claimed class, but **none of it is authenticated at that stage**: the class lottery needs the class target, the
signature needs the bond's key, both in state.

**10.2 Header-level floor invalidation (the direct answer, not built).** Make a floor-lane block invalid at the header stage when
the chain is not Idle, so it never enters the DAG. It needs a floor state at the header stage. Two findings against building
it as a recursive header-level machine:

* **Header-claimed events are forgeable.** The only REAL signal a header carries is its own claim. Cheap forged REAL-claiming
  headers (their bodies are refused later) would drive the machine — holding the floor off at will (a denial of the fallback), or
  flipping it at will — at the price of a header's proof of work, which for an attempt-lane header is small. A state machine
  must not be driven by data nobody has checked.
* **A state-based cooldown has unbounded memory.** The header stage derives its values from a bounded window of ancestors (ADR-0142
  §6a: a pruned node and an archival node agree because the cursor is a function of a window). `last_probe_end` reaches back as far
  as the last unanswered probe — arbitrarily far — so a node that starts at the pruning point cannot rebuild it from headers. The
  fold-level machine has no such problem because its state is *rooted* and travels in the carriage against a committed root.
  A header-level one would have to commit its state in the header (a header-hash change) and validate each header's value as the
  step from its parent's — which fixes the memory, and not the forgery.

**10.3 Class-aware colouring (the structural alternative; the next fence, lane RF8's RFC-0008 alternative to its merge-admission design).**
ADR-0105's `LaneColoring::Weighted` already makes heartbeats invisible to a bonded candidate by lane. Extending it so floor-lane peers
are invisible to a REAL candidate needs no state machine, no probe, no producer hold and no anchor duty: the floors keep flowing, so the
anchors (the round seed, the jury, lane A's binding), the clock and the bonded stretches are exactly as they were, and REAL BLUE stops
depending on anyone's compliance — P2's measurement of the dependence is 00.2's table (one non-compliant producer at a tenth of the floors
takes REAL BLUE from 97 % to 72 %). A REAL candidate's class is in its commitment, readable at the header stage; the floor peers would be
told apart the same way. The floor, for its part, would still yield to REAL blues as a heartbeat yields to bonded ones.

**The open question, which decides whether it is safe: what unverified BLUE does it grant a header-claimed REAL attempt once floors no
longer compete with it?** A header can claim a REAL class without anyone having checked the claim at that stage (the class target, the
bond's registration, the room and the lottery are state; the envelope's signature against its embedded key is the one thing the relay path
checks statelessly). Today such a header is coloured classically against the floors around it, and at `ghostdag_k = 1` most of them turn
RED. With floors invisible to it, a forged REAL-claiming header would be BLUE against everything but other REAL-claiming headers — and
BLUE is what lets a block's header-stage weight (the constant every attempt-lane header adds, the fork-choice attack review's verdict 1)
count towards GHOSTDAG's blue work and the selected chain. Three things to settle before it ships: (1) how much blue work an adversary
gains per unit of proof of work by minting REAL-claiming headers with no valid body (bounded by the per-header work, by `k` among the
forgeries themselves and by the merge-depth floor `Weighted` already applies, but not yet measured); (2) whether the immunity should be
conditioned on something a header can carry and the header stage can check — the claimed class being one the carried registry root lists,
or the bond being named in a header-committed bond set — at the price of header bytes; (3) the effect on the selected parent, since a
REAL attempt then out-weighs a floor chain it used to lose to. It also touches every site that computes GHOSTDAG (the pruning proof's
two included, which take the colouring as a constructor parameter), so it needs its own fence, its own drill and a fork-id.

**10.4 Re-anchoring inference work (the cure for the slow producer itself).** A REAL attempt's execution is bound to the template
it was drawn on (ADR-0072: the ticket is the execution), so a chain that moves for three slots under a draw makes its block a late
side block — the slow-producer problem that A″ mitigates by silence. A producer could carry a finished inference to a newer tip
if the job anchor came from a coarser clock — the slot — instead of the exact parent set. That touches ADR-0072's anchor, ADR-0152
J-1's weight cap and the replay rule; it is the structural cure and is not designed here.

**10.5 One clock, carried by the work.** FALLBACK-V1 (§5) retires the heartbeat: the DAA is ticked by the model's attempt where
there is one and by the bonded fallback block otherwise, a single model-carried clock. It belongs in the same fence as whichever
of 10.2–10.4 is chosen, because the fallback block's idle-only rule reads the floor state this ADR introduces.

**What is decided today:** 5,300 ships A″ + B as built, with the producer hold on by default everywhere a floor is mined. §10 is
the agenda of the fence after it.
