# PALW mainnet audit, 2026-09-06 — the two-tree pass

## 要旨 (日本語)

**結論: 今日 mainnet card を鋳造してはならない。** そもそも今日は鋳造「できない」— `PALW_MAINNET_GENESIS_ARTIFACT_ROOT` を埋めた `mainnet_shipped_params()` は 09-05 監査が追加した ambient-target ゲートで panic する (params.rs:8186)。fail-closed なので実害は無いが、儀式手順書が指す検証コマンドはその panic を踏まない定数を pin しており、card 組立を実行するテストは一つも存在しない。

鋳造前に真でなければならない三点。

1. **応答者のいない court を武装しない。** `mainnet_card_base_v1` は `palw_da_court` を無条件に `always()` で武装する (params.rs:8241) が、`disclose_trace_event` を実装する backend は 3 家族中 1 つだけ — dense (489‰) と Qwen3.6 の全 claim が、bond 一つを持つ他人の告発 1 通で genesis から slash 可能。同じ形が k-ary court の fused Terminal にもあり (court-adjudicability-1 / prior-open-closure-2)、こちらは証拠ゼロの `CourtOpened` が round 0 の沈黙だけで有罪にする。`validate_palw_v2` に「その card が登録する全家族が応答できるか」という consensus 可視の述語が要る。
2. **無料で買える行と無料で買える quanta を塞ぐ。** attempt lane の Layer-0 digest は bond 無しでハッシュだけで grind でき、しかもそれが `bits` を動かす計上行になる (weight-and-time-1)。free-prompt では seat が抽出する interval 数を被告自身の job が決めるため、2 回の forward pass が 63 quanta を買う (freeprompt-claims-1)。
3. **1 ブロックで全ノードが落ちる算術を直す。** PQ ネットでは標準 UTXO の plurality が 2 になり、`calc_storage_mass` が 0 除算で panic する (mass/mod.rs:409)。攻撃コストはブロック 2 個と 4 sompi。

**前回との差。** 前回が閉じたと記録した項目のうち O-12 は card で武装されない fence の裏にあり実質未閉、O-8 は前回自身の修正 (card の fence 表) によって初めて card 上で発火するようになった。さらに ADR-0087/0088/0089/0090 の model market 枝を mainnet 監査が読んだのは今回が初。

---

## Scope and method

**The question.** Every candidate finding was asked the same thing the 2026-08-30 and 2026-09-05
passes asked: *would this hurt on a mainnet carrying real value?* "Mainnet" means a **carded**
mainnet — `PALW_MAINNET_GENESIS_ARTIFACT_ROOT` and `PALW_MAINNET_GENESIS_BONDS` set, so
`mainnet_shipped_params()` (params.rs:8130) assembles through `mainnet_card_base_v1`
(params.rs:8215) and `palw_rc_arm_phase1` (params.rs:8273).

**Two pinned, read-only trees.** Neither was edited during the run, deliberately: the 2026-09-05
pass corrupted its own results by fixing the tree its refuters were reading, so a refuter's "the
guard exists at params.rs:1570" was reporting that audit's own fix rather than an acquittal.

| tree | branch | tip | what it is |
|---|---|---|---|
| COURT | `palw-t11-da-court-market` | `3c6d6747` | DA court, private prompts, every 2026-09-05 fix |
| MARKET | `palw-adr0088-0089-impl` | `8146e659` | ADR-0087/0088/0089/0090 model market + EVM |

They diverge at merge-base `32c772cc`. **MARKET has never been read by a mainnet audit** — it did
not exist when the 09-05 pass ran, so none of its findings can appear on that pass's open list.

**Shape.** Nine deep finders, one per lane, each the *only* coverage of its lane → one strong
refuter per lane reading the whole lane at once, defaulting to refuted → a second,
differently-angled lens (does it pay at mainnet scale / is the consequence the one claimed) on the
CRITICAL and HIGH survivors, capped at ten → this report. **29 agents in total** (9 + 9 + 10 + 1).

**Lanes.** money-mint · weight-and-time · court-adjudicability · freeprompt-claims · p2p-dos-ibd ·
carding-activation · crypto-tx-validity · model-market-evm · prior-open-closure.

No agent built, ran or edited anything. Every cost figure below is derived from constants and
instruction counts in the source, or from measurements already recorded in ADR-0084; nothing was
measured in this pass.

### What the leaner shape cost

The 2026-09-05 pass ran 18 finders × 3 refuters = **168 verify agents**, and its own method section
called that "roughly twice what the yield justifies"; a session limit then killed 52 agents mid-run
and left six dimensions unjudged. This pass is about **one sixth** of that fan-out.

The reduction was cheap on the verification axis and expensive on the coverage axis, and the two
should be scored separately.

* **Verification held up.** Of ~30 findings, one was killed outright (p2p-dos-ibd-2), one was
  demoted by the second lens (freeprompt-claims-2, HIGH → MEDIUM), five were downgraded by their
  refuter, and six of the ten second-lens findings were materially corrected — almost always on
  *price* or *duration*, never on mechanism. The prior pass killed 40% of its findings; this pass
  killed ~3%. That difference is not evidence that this pass's refuters were softer: the prior
  pass's own conclusion was that most false findings die to the guard lens alone, and one strong
  refuter reading a whole lane at once applies exactly that lens with more context than three
  refuters reading one finding each. The demotions and corrections are concentrated in the payoff
  axis, which is precisely what the second lens was added for and what a per-finding refuter is
  worst at.
* **Coverage did not.** Single coverage per lane means a lane's blind spot is the audit's blind
  spot, and this pass produced a clean instance of that: the p2p finder listed
  `request_pruning_point_snapshots.rs` as read end to end and still missed the largest thing in its
  lane — an unauthenticated p2p flow that runs *two* full PALW-state materializations per 40-byte
  request (below, HIGH). The refuter found it only because it re-read the whole lane. Nine lanes
  also cover less ground than eighteen dimensions: EVM execution proper, the ADR-0088 proposal /
  evaluation / version-eviction arithmetic, the read-precompile ABI encoders, host security, the
  pruning proof and executor determinism went unread this pass (see the last section).

**Verdict on the shape:** one strong whole-lane refuter plus a payoff lens on the top of the list is
the right verification budget. The saving should be spent on *more lanes*, not on more verifiers.

---

## Verdict

**A mainnet card is not mintable today, and would not be safe to mint if it were.**

It is not mintable in the literal sense: with the artifact root set, `mainnet_shipped_params()`
panics on the genesis-`bits` gate the previous audit installed (carding-activation-3). That is
fail-closed and is the smallest of the problems.

Three things must be true before a mint, in this order:

1. **No court is armed over a family that cannot answer it.** Today `mainnet_card_base_v1` arms
   `palw_da_court` *unconditionally* while two of three shipped backends have no
   `disclose_trace_event`, and arms `palw_kary_court` whenever the dense tier is pinned while no
   binary in the tree constructs the `CourtAttnRootClaimed` its fused Terminal clocks the responder
   for. Both convict honest producers by silence, for the price of one bond. The repair is a
   responder-coverage predicate in `validate_palw_v2` beside the SA-3 window check
   (params.rs:1920-1943) — a consensus-visible fact, not the node-local `supports_court()`, which
   the tree already says is "exactly the wrong shape" (palw_e2e_adjudicability.rs:7-18) — plus the
   three-line delegations that give the a16 and Qwen3.6 backends a real responder.
2. **Work that moves the clock or mints quanta must cost work.** The attempt lane's Layer-0 digest
   is a hash grind with no bond, no class and no state read on the header path, and it is a
   `bits`-priced difficulty row; the free-prompt seat draws its verification sample from a count the
   accused producer wrote. Either one alone converts the chain's advertised inference price into a
   hash price.
3. **`calc_storage_mass` must stop dividing by zero.** One attacker-built block exits every
   validating node, permanently, for two blocks of work and four sompi.

Everything else on the list is smaller than those three, and several of the MEDIUMs are launch-day
process items (the card ceremony's own arithmetic, the doc the ceremony is planned from) rather than
attacker-reachable defects.

**Fenced vs live.** Of the twenty-five findings below, **fourteen are live from genesis on a card
with no fence above them** (weight-and-time-1/2/3, court-adjudicability-1/2, freeprompt-claims-1/2/3/4,
crypto-tx-validity-1/2, prior-open-closure-1/2/3, money-mint-1/2/3, p2p-dos-ibd-1,
carding-activation-1/3/5). Six are behind fences no card arms — the entire model-market set — and
are MEDIUM or LOW by that rule alone; they matter because ADR-0087/0088/0089 all state activation,
never regenesis, as their arming model, so those fences *will* be armed on a live chain.

---

## Findings by severity

### CRITICAL

---

#### C-1. A bondless party grinds attempt-lane blocks at hash cost, and they are `bits`-priced difficulty rows

*(lane: weight-and-time · finder + refuter + second lens · CONFIRMED by both)*

**Location.** `consensus/src/pipeline/header_processor/pre_ghostdag_validation.rs:150-156, 225-283`
(the whole header-stage PALW rule set) · `consensus/core/src/palw_attempt_v2.rs:323, 324, 359`
(`trace_root`, `output_root`, `execution_root`), `:513-521` (`execution_commitment_v3`), `:547-556`
(`l1_tag_v2`), `:213-219` (`palw_nonce_bucket_v1`) · `consensus/pow/src/lib.rs:386-410, 561` ·
`consensus/src/processes/difficulty.rs:331-346` · `consensus/src/processes/window.rs:311-329` ·
`consensus/src/pipeline/virtual_processor/processor.rs:1553-1563, 7046-7057`.

**Mechanism.** The header-stage PALW rule set is exactly two functions and neither reads chain
state: `check_palw_commitment_shape_at` (version, envelope decodes, `pwu != 0`, non-empty pubkey,
signature *length*) and `palw_carriage_stateless_v1`, whose signature check is under the **carried**
key — its own comment says "whether the carried key IS the named bond's key is admission item 2's
stateful question" (pre_ghostdag_validation.rs:266-268). `execution_commitment_v3` borsh-serializes
the whole attempt with only `challenge` blanked, so `trace_root`/`output_root`/`execution_root` sit
inside the priced bytes and nothing pins them at header time; `l1_tag_v2` is a free CPU expansion.
`consensus/pow/src/lib.rs:561` is then `pow_512 <= target_512` from `header.bits`. A second, cheaper
axis needs no field edit at all: `execution_anchor_v3` keys on `nonce >> 22`, so stepping the nonce
by 2²² gives 2⁴² distinct anchors — hence 2⁴² distinct digests — per template, with the attempt
bytes and the signature untouched. `difficulty.rs:346` is
`new_target = average_target × measured_duration / (120_000 × counted_rows)` with `.min(max_difficulty_target)`
as the *only* clamp, i.e. no bound in the tightening direction, and `window.rs:311-329` puts reds in
the window.

**Reach.** Peer block → `validate_header_in_isolation_sans_pow` → `check_pow_and_calc_block_level` →
`check_palw_commitment_shape_at` + `palw_carriage_stateless_v1` → `check_difficulty_and_daa_score` →
stored, relayed, merged. The only path that would refuse it, `palw_v2_check_attempt_admission`, runs
in the virtual processor and its two verdicts are `StatusDisqualifiedFromChain` (processor.rs:1560)
and a `skips` log entry (processor.rs:7055). Neither invalidates the block or removes the row.

**Scenario and price.** One ML-DSA-87 keypair, commodity CPU, no bond, no class, no stake. Per
draw: one borsh of ~200 bytes plus four blake2b — order 3 µs, so 10⁵–10⁶ draws/s/core against an
honest producer's *one draw per inference*. At equilibrium the attacker publishes ~1 block per
120 s and signs it once (~1 ms). There is no width bound on the attempt lane — only the heartbeat
lane got one (`PALW_HEARTBEAT_MAX_PER_MERGESET = 4`, pow_layer0.rs:575), which is exactly the guard
the free-row lane needed and did not get. Honest share of production is I/(H+I): at H/I = 10⁵ a
bonded producer's expected wait moves from 120 s to ~139 days.

**Mainnet impact.** Live from genesis on a card and **not fenceable** — `palw_difficulty_priced_rows`
and `palw_receipt_rows_unpriced` are `always()` in `mainnet_card_base_v1` (params.rs:8222-8223) and
the attempt lane is priced *by design* (`algo_id_is_priced_by_bits_v2`, pow_layer0.rs:405-412), so
no dormant fence exists to arm in response; the answer is a code change and a flag day. The tree
carries the empirical proof of the end state: params.rs:8320-8324 records that on Relaunch 5f five
free-row heartbeat emitters "tightened bits from p = 0.5 to p = 1.5e-3 in 826 blocks and no bonded
lane could win again". ADR-0083 closed *that* by taking the heartbeat lane out of the row count; the
identical arithmetic re-enters through the lane the repair declares counted. The chain does not
halt (the heartbeat target is the constant 2⁻²⁴), it degrades to heartbeat-only with a frozen PALW
state — and because `safe_frontier` advances only when a claim resolves (palw_state_v2.rs:6695) and
`pruning_ceiling_v2` *is* the safe frontier (palw_fork_authority_v2.rs:70-77), every node stops
pruning for the duration.

**Refutation history.** The refuter tried all four axes and could not kill it, and found the tree
diagnosing this in writing at `ghostdag/protocol.rs:407-415` — a 2026-09-05 "mainnet audit" comment
saying the digest "is GRINDABLE at hash cost alone … One inference buys an unbounded nonce search"
— which was acted on by denying the lane `level_work` and leaving the same digest as the network's
`bits`-priced rate control. It added the nonce-bucket axis and noted `palw_lane_blue_work_v1` gives
every attempt block a flat 2²⁰ (protocol.rs:437-441) while `palw_tip_weights_v1` returns `None` on a
V2 network (processor.rs:11234-11236). The second lens confirmed and *built a refutation that
failed*: it hoped attacker blocks would be popped and discarded by `sink_search_algorithm` forever,
but `protocol.rs:189-207` seeds `mergeset_blues` with the selected parent, so a block's own work is
paid to its children and an attacker block is never strictly heavier than the sink — it is always
merged. The second lens corrected two over-claims: the 180× DAA acceleration is ramp-only, and the
refuter's "chain weight is proportional to the count of attempt blocks" does not buy the sink,
because every such block is `StatusDisqualifiedFromChain`.

**Relation to prior work.** O-5 names `output_root` as a free field but assumes a *bonded* producer
paying in escrow after the panel reacts, and names lottery draws as the payoff. Here no bond, no
class and no claim exist, so the escrow/panel/court backstop never engages, and the payoff is the
global difficulty retarget. O-13 names the constant blue work; this is the other half.

---

#### C-2. A stranger convicts any dense-tier producer with no evidence — the fused Terminal, and round-0 silence

*(lane: court-adjudicability · finder + refuter + second lens · CONFIRMED by both)*

**Location.** `consensus/core/src/palw_state_v2.rs:7025-7030` (the fused Terminal →
`AwaitDisclosure` remap), `:7149-7152` (`turn_can_still_move` / `rung_fired`), `:7213-7215` (the
round-0 mercy), `:7231` (`void_and_slash(CourtFraud)`), `:5988-5997` (`void_and_slash` = `void_claim`
+ `slash_bond`) · `consensus/core/src/palw_bisect.rs:568-575` (`agree` is an unchecked free bit),
`:592, 938` (Terminal → Responder) · `consensus/core/src/palw_court_v2.rs:308-372`
(`validate_court_opened_v2` asks for no evidence) · `kaspad/src/palw_panel.rs:2381-2470` (the only
close the responder can build), `:3576` (`CourtAttnRootClaimed` exists in kaspad only as a string) ·
`consensus/core/src/config/params.rs:8215-8218` (the card arms `palw_kary_court` when the dense tier
is pinned).

**Mechanism.** Two versions, and the cheaper one is the one nobody named.

*Unsteered.* `sweep_court_deadlines` charges `Responder` for silence at plain `AwaitDisclosure`, and
the round-0 mercy at :7213-7215 is conditioned on `!class_holds_weight`. A card gives the model
tiers ~978‰, so the mercy never applies to them. An evidence-free `CourtOpened` — all
`validate_court_opened_v2` asks for is a claim in `window_challenge`, an Active bond at the
collateral floor, `space_size >= 2` and a signature — convicts any dense- or hybrid-tier producer
that cannot answer one rung inside `turn_deadline_daa` (42 DAA). **One transaction plus a 42-DAA
wait.**

*Steered.* `apply_verdict` reads `msg.agree` as a bare boolean and nothing compares it to anything,
so the **challenger alone** chooses the terminal leaf. `processor.rs:5752-5757` refuses any
`CourtOpened` whose `space_size != court.max_step_leaf_count()` = 2²⁶, against a dense canonical job
of 6,630,544 leaves — ninety percent of the ladder is padding. Twenty-six `agree = true` verdicts
drive `lo` to 2²⁶−1; the responder walks in willingly because `base0_bisect_prefix_state_v1`
saturates (`legs.rs:489-495`) so every padded midpoint is a well-defined distinct value. At
Terminal the panel's only arm calls `refutation_for_index`, which resolves through
`canonical_step_coordinates` and returns `Err("leaf N is not a main step coordinate")`
(backend.rs:122-126). The fused remap at :7025-7030 then rewrites `Terminal` to `AwaitDisclosure`
on the theory that the responder owes a `CourtAttnRootClaimed` — an object no binary outside
`palw_state_v2`'s test module constructs.

The padding is **not load-bearing**: the tree's own comment at palw_state_v2.rs:7005-7014 records
that on a fused site an `Arithmetic` whole-row close is refused by `check_close_cost_v2` and an
`AttnDissection` close is refused `NoDissection`, so *every* terminal leaf of a fused class is
undefendable until the responder ships. A fix that rejects out-of-enumeration indices closes
nothing.

**Price (second lens).** Attacker: one Active bond at `MIN_COLLATERAL_SOMPI = 400,000` sompi, plus
`claim.reserved` (= `pwu_per_inference × SLASH_VALUE_PER_PWU`; for the dense row 6,630,544 × 5 =
33,152,720 sompi) reserved under the 500‰ exposure ceiling — i.e. ~0.66 MSK of *locked, never
at-risk* collateral per concurrent session, released on every session removal
(palw_state_v2.rs:5749-5766). Throughput: `court_turn_deadline = 42` gives
`palw_max_concurrent_court_sessions_v1` = 42 chain-wide, and crucially the rungs do **not** spend
the block's single court slot — `palw_court_move_spends_the_slot_v1` (:3621-3628) matches only the
three `CourtAttn*` dissection objects — so all 42 sessions advance every block. Steady state ≈ 42
convictions per ~95 blocks, **~300+ per day**, for ~28 MSK of returned capital.

**Payoff.** `void_and_slash` **burns** (`slashed.collateral -= debit`, :5975-5979) and pays the
challenger nothing, so this is destruction, not extraction. It also destroys the escrowed reward:
escrow releases only at `Final` (:2001-2002). Two convictions take a floor-collateral producer
below `min_collateral_sompi` and out of every eligibility check. A competitor clears the tier ADR-0068
gives most of the cadence to, for capital it gets back; a short-seller burns ~100 MSK of collateral a
day.

**Mainnet impact.** Armed by the act of pinning the dense tier — `PALW_MAINNET_QWEN25_A16_ARTIFACT_ROOT`
is all zeros in this tree (params.rs:5873) and params.rs:11999-12000 asserts that with nothing
pinned mainnet is byte-identical to what it shipped as. **Nothing is exploitable today; the launch
arms it.** That is what makes it a launch blocker rather than a live incident.

**Refutation history.** The refuter confirmed every link and corrected three points: the fold (not
the acceptance function) demands the challenger's bond be Active and above the floor
(palw_state_v2.rs:8929-8944) — the two spellings of the same admission rule disagree and only the
stricter is on the live path; the concurrency cap is chain-wide, not per bond; and the payoff is
destruction, not a bond grab. The second lens confirmed, priced it, and corrected the refuter's
throughput estimate upward by ~50× (the slot predicate does not match ladder rungs) and the
finder's framing downward (the padding is not what makes it possible; silence at round 0 is cheaper).

**Relation to prior work.** This is O-8, and everything about its reach is new. O-8 reads as a
passive liveness gap waiting on a feature ("the missing half is the responder's software, a feature
rather than a patch"); it is in fact a steered, evidence-free, funded attack that a card arms.

---

#### C-3. Two forward passes buy the free-prompt lane's 63-quantum jackpot, because the seat's sample denominator is the accused producer's own job

*(lane: freeprompt-claims · finder + refuter + second lens · CONFIRMED by both)*

**Location.** `kaspad/src/palw_panel.rs:4118-4119` (the seat's counts), `:2661` (`ctx =
resolved.fp_job_context_v1(&material.job)`), `:1236-1247` (`fp_job_material_for_claim` accepts a
payload on class_id + bond alone), `:2669` (`break 'verdict Some(Valid)` short-circuits the
re-pricing arm at `:2733`) · `misaka-palw-base0/src/qwen25_a16_backend.rs:1513-1522`
(`decode_tokens_executed: job.decode_token_limit`), same at `qwen36_backend.rs:1464`,
`backend.rs:651` · `misaka-palw-base0/src/fp_interval.rs:286-296, 334-364, 1877-1894` ·
`consensus/core/src/palw_state_v2.rs:9616, 9690-9697, 9814` ·
`consensus/core/src/palw_freeprompt_v3.rs:1067-1085` ·
`consensus/core/src/palw_step_leg.rs:353, 365-410` (prover-supplied siblings).

**Mechanism.** `work_leaves` is the whole price: `quanta = fp_quanta_v3(work_leaves, quantum, cap)`
and `pwu = quanta × quantum`, and the arm destructures `decode_tokens_executed: _`. The only chain
bound on `work_leaves` is `!= 0` and `<= max_step_leaf_count` (2²⁶). The design's answer is the
panel — `PalwSeatDutyV2`'s own doc states the attack ("a producer may declare a hundred-thousand-token
job, serve a one-token material whose roots are genuinely that material's") and names the defence as
"a seat re-prices what it actually executed" (palw_producer_v2.rs:521-531). The re-pricing that runs
compares the opening against `binding.step_leaf_count`, which the producer authored, so it reduces
to "the producer's declared context implies the producer's declared leaf count". The real defence is
the **sample**, and its denominator is producer-chosen too: `PalwFpChainCountsV1` is built from
`fp_job_context_v1(&material.job)` where `material` is the payload the accused served, and every
shipped `fp_job_context_v1` writes `decode_tokens_executed = job.decode_token_limit`. That type's
own doc states the contract being violated: "a seat that read them off the served capture would be
letting the accused choose the number of intervals" (palw_fp_seat.rs:151-160). With
`PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1 = 1` a served job declaring `decode_token_limit = 2` yields
`interval_count = 1`, so the draw is `[0]` — and interval 0 is the unique interval with
`anchor_covered_call(0) == None`, so the ADR-0082 D9 seat-side checkpoint recompute
(palw_panel.rs:4147-4186), the one check that would compare the two contexts, never runs. The
unopened ~510/512 of the step tree costs nothing because `step_range_opening_root_capped_v1`
rebuilds the root from prover-supplied siblings.

**Price (second lens).** Dense row: quantum = 6,630,544/8 = 828,818 leaves; 63 quanta → pwu
52,215,534 → reserved 261,077,670 sompi, so the 500‰ ceiling demands 5.22 coins of collateral —
*reserved*, released at Final. Compute: 2 forward passes against the 512 an honest 63-quanta claim
pays. Payoff: up to 63 receipt blocks, and a receipt block costs **no hashing at all**
(pow_layer0.rs:407: "the receipt lane (7) has no target at all"), so each spent quantum is a block
minting the year-1 subsidy of 370,468,345 sompi — ~233 coins per claim, ~45× the entire collateral,
recycled every bind+receipt+challenge = 2,400 DAA (~6.7 h). Note the misdeclaration is not merely
over-payment: `fp_quanta_v3` floors, so an honestly declared 2-call job yields **zero** quanta and
`ZeroQuanta` — inflating the declaration is the only way this shape pays at all.

**Reach and consequence.** Ordinary `SUBNETWORK_ID_PALW_FP_COMMITMENT` transaction. `apply_receipt_spend`
adds `per_quantum` to `state.safe_weight` (palw_state_v2.rs:9814) — fork choice's second key — and
to `produced_pwu`, which feeds the class retarget the attacker is exploiting. The honest
counterweight never fires: a court would convict, but the slash is burned rather than paid to the
challenger, the five seats that hold the evidence log "drew [0] of 1 interval(s)" next to a 52M-leaf
price and compare the two numbers nowhere (palw_panel.rs:4133-4137), and under privacy mode 2 —
`palw_panel_da` armed from genesis by the card (params.rs:8238) — no non-seat holds the material at
all.

**Mainnet impact.** Genesis-armed on a card; nothing here is behind a dormant fence. The result is
~98% of block production across the two model classes at ~1/256 of the advertised inference cost.
ADR-0044's "one inference, one ticket" and ADR-0074 D5's "leaves are the work" are both false on
this path.

**Refutation history.** The refuter confirmed every link and corrected two over-claims: a *third
party* cannot swap another producer's counts (the binding pins `job_context.job_id` to
`fp_job_id_v3(&material.job)`), and the 1/256 figure is conditional on the family's checkpoint
interval being 1. It also noted the whole-capture arm that *does* re-price is short-circuited by
`break 'verdict Some(Valid)`, so the fix is one line: run the price check on the interval path too,
against the seat's own context. The second lens confirmed and made it **worse**:
`fp_job_material_for_claim` admits a payload on class_id + executor_bond + privacy alone while its
own doc asserts "the job whose id is the claim's anchor", and the duty carries neither job id nor
prompt hash — so the attacker need not publish the tell-tale `decode_token_limit = 2` on chain. A
commitment that is internally consistent and byte-indistinguishable from an honest widest job, plus
a served 2-call stub, yields the same draw. **That unenforced job binding is arguably its own
finding.** The second lens also narrowed "reorg weight purchasable": `safe_frontier_blue_score` is
fork choice's first key and a private fork collects no receipts, so the claim must be public and
panelled — what is purchasable is the public chain's issuance and `safe_weight`.

**Relation to prior work.** The 09-05 unjudged list has "a claim's price is read from a job context
the chain never sees and no seat compares to the job (fp_interval.rs:1849)". New: the *sample*
denominator is producer-chosen too, interval 0 is the unique unanchored interval, the payoff is
fork-choice weight and issuance, the ADR-0077 D8 arm short-circuits the older random-leaf sampler
that would have caught it, and the sizing is quantified against the shipped constants.

---

#### C-4. Every standard PQ UTXO has storage plurality 2, so a two-input dust transaction divides by zero and exits every node

*(lane: crypto-tx-validity · finder + refuter + second lens · CONFIRMED by both)*

**Location.** `consensus/core/src/mass/mod.rs:407` (`let mean_ins = sum_ins / ins_plurality;`),
`:409` (`ins_plurality.saturating_mul(storm_param / mean_ins)`), `:64-82` (`utxo_plurality`), and
the stale invariant at `:63` · `crypto/txscript/src/script_class.rs:120-127` (the 69-byte
P2PKH-ML-DSA-87) · `consensus/src/processes/transaction_validator/tx_validation_in_utxo_context.rs:52, 158-166`
· `consensus/src/pipeline/virtual_processor/utxo_validation.rs:368, 384, 1034` ·
`core/src/panic.rs:29`.

**Mechanism.** `utxo_plurality` is `(95 + spk.len()).div_ceil(100)`. The only standard kaspa-pq send
template is 69 bytes, so **every standard UTXO has plurality 2** and an `EvmDepositLock` has 3.
Upstream's line 63 — "The choice of 100 bytes per unit ensures that all standard SPKs have a
plurality of 1" — is the invariant that made `sum_ins >= ins_plurality` hold, and widening the
outpoint id to `Hash64` (ADR-0008) killed it silently while leaving the comment in place. The
relaxed-formula escape at `:385-401` is unreachable for any multi-input transaction on a PQ
network: `outs_plurality == 1` needs a ≤5-byte script, and `outs == 2 && ins == 2` is exactly one
input and one output. Worked case: two 1-sompi inputs (`ins_plurality = 4`, `sum_ins = 2`), one
1-sompi output — `harmonic_outs = 4e12` computes fine through the `checked_mul`s so the graceful
`MassIncomputable` escape is *not* taken, `mean_ins = 0`, panic. Rust integer division by zero
panics unconditionally, and the panic hook runs at raise time, before unwinding, so `process::exit(1)`
fires regardless of rayon or `catch_unwind`.

**Reach.** The mempool is *not* the vector — `is_transaction_output_dust` runs in isolation before
the contextual-mass computation. A miner puts the transaction straight into a block built outside a
kaspad instance. Block B2 → `calculate_utxo_state` / `verify_expected_utxo_state` with
`TxValidationFlags::Full` → `check_mass_commitment` → `calc_contextual_masses` → panic → exit.

**Scenario.** Two blocks of PALW work (a 2²⁴-hash heartbeat is seconds of CPU) and ~4 sompi. Every
peer that validates B2 dies, including the peers that would have rejected it, because the panic
happens inside the validation that would have produced the rejection.

**Mainnet impact.** Whole-network liveness kill at will, on the base money path, with no fence in
front of it: `MAINNET_PARAMS.storage_mass_parameter` is `STORAGE_MASS_PARAMETER` and the card
carries `pq_enforcement: Consensus` (params.rs:8729), so the plurality-2 condition holds from block
one. **The second lens found the consequence is worse than "node exits":** B2 passes
body-validation-in-isolation (the block-level cap sums the *committed* `tx.mass()`, which the
attacker sets small) and is therefore already stored and relayed before the panic; on restart the
node re-resolves virtual state over the same mergeset and re-panics. A persistent poison block that
bricks every node across restarts, escapable only by a code patch plus manual DAG surgery. It is
equally live on testnet-11 today; a card only changes what an outage costs.

**Fix.** At the arithmetic-input path, not by adding a dust floor: make the input branch checked
like the harmonic one (`None` → `MassIncomputable`). Before a mint, sweep every caller of
`calc_contextual_masses` — `wallet/pskt/src/pskt.rs:460` and `consensus/src/consensus/mod.rs:1195`
both expose it, and a fee-estimation RPC on attacker-shaped inputs would be a second crash surface.

**Refutation history.** Confirmed on all axes by both lenses. The second lens narrowed the
"accidental coinbase dust" secondary: a single tiny UTXO swept alone takes the relaxed path and does
not panic, so the accidental trigger needs ≥2 dust inputs in one transaction that also bypasses the
mempool dust guard.

**Relation to prior work.** New. The 2026-09-05 audit has no mass, plurality or storage-mass item
anywhere.

---

#### C-5. A card arms `palw_da_court` unconditionally while two of three shipped backends have no disclosure responder

*(lane: prior-open-closure · finder + refuter + second lens · CONFIRMED by both)*

**Location.** `consensus/core/src/config/params.rs:8241` (the arming) against `:1023-1033` (the
field's own prohibition) · `consensus/core/src/palw_backend.rs:257-266` (the defaulted responder) ·
`misaka-palw-base0/src/qwen25_a16_backend.rs:1016` and `qwen36_backend.rs:993` (the two impl blocks
that do not override it; only `backend.rs:484` does) · `kaspad/src/palw_panel.rs:2556-2574` (`Err`
→ `continue`, nothing filed) · `consensus/core/src/palw_state_v2.rs:9420` (`DefaultAccused`),
`:8132-8134` (the sweep), `:5988-5997` (`void_and_slash`).

**Mechanism.** Twelve lines above the arming site, the field's own doc reads: "**DO NOT ARM THIS
WITHOUT A DISCLOSURE RESPONDER IN THE FIELD.** … Armed with no responder, every accusation succeeds
and every producer is slashable for the price of one bond — strictly worse than the captured-panel
defect this ADR was written to close." The commit's justification (params.rs:8234-8236) says the
court is arm-able "now that a seat answers accusations". That is true of exactly one family:
`disclose_trace_event` is defaulted to `Err("this family has no data-availability responder")`, and
of the four implementors of `PalwExecutionBackendV1` only `Base0Backend` overrides it. The dense
graph-v5@512 row a card pins resolves to `Qwen25A16Backend`. The panel's DA half logs
`warn!("cannot open accused event …")`, bumps `court_stalls`, and `continue`s.

**Price (second lens, corrected from the finder).** `palw_da_accusation_exposure_v2` =
`min(pwu, 400,000)` sompi, and `reserve_accuser_exposure_v2` tests the sum against
`collateral × 500‰` — so a bond at the floor funds **zero** large-claim accusations and the true
price is **800,000 sompi per concurrent slot** (the finder's 400,000 is wrong by 2× in the
attacker's disfavour). One coin of locked collateral buys 125 simultaneous sessions. Nothing is at
risk: the sweep returns the reservation in full, and the charge that would punish a false accuser
can only fire if a disclosure lands. The producer had to hold ≥10 × pwu to pass admission item 8
and loses 5 × pwu per accusation, so **two accusations empty a minimally funded model-tier bond**.
No per-block throttle exists: `palw_court_move_spends_the_slot_v1` does not match `DefaultAccused`,
which is why this, not the k-ary path, is the cheap one.

**Counterplay: none in the shipped software.** SA-3 binds a disclosure to the *claim's* bond key, so
no third party can answer; `resolve_backend` dispatches by class id and the raw-capture fallback
refuses any backend whose row is not the capture's own class (palw_panel.rs:3967), so the floor's
working responder is unreachable for a graph-v5 or graph-v3 capture on both lanes.

**Consequence.** An accusation is only admissible on a non-terminal claim, so an accused claim never
reaches `Final` — its escrow is never paid and its weight never matures — *on top of* losing
`claim.reserved`. The two tiers are unproducible from block one and pay to try. The slash is
burned, so the payoff is share capture: ADR-0054 decays a quiet class's share back into the
liveness floor, and a floor-class operator converts the model tiers' 978‰ into its own.

**Mainnet impact.** The arming is **unconditional** (unlike `palw_kary_court`, which is gated on
`dense_tier_pinned`), so this bites a card that pins only Qwen3.6 as well as a full card. testnet-11
and devnet leave the fence dormant, so this is a defect *only* on the value-carrying chain. It is
not fenced away; it is fenced *in*.

**Worse than the write-up, from the second lens:** both backends declare `supports_court() == true`
for their registered rows (qwen25_a16_backend.rs:1307, asserted true for graph-v5@512 at :2563;
qwen36_backend.rs:1265), while that method's own doc defines it as covering exactly
`disclose_trace_event` and `bisect_prefix_state`. So the boot-time honesty signal *and* the
weight-granting E2E drill (`drill_family_v1`, which refuses only on "take a court's turn")
both certify these families as court-capable without ever exercising the DA responder — the audit3
H4 shape the trait doc says it exists to prevent, reproduced one method over.

**Recoverability.** The fix is producer-side and non-consensus: `Base0Backend::disclose_trace_event`
is already generic over the shared retention codec (`base0_material_decode_any_v1`) that both other
backends' `capture_shape` and `bisect_prefix_state` already call, so a three-line delegation in each
impl closes it with no re-mint. A launch NO-GO, not a permanent trap.

**Relation to prior work.** Not on the 09-05 list at all — it is **created by that pass's own
follow-through** (`f7363db9` / `mainnet_card_base_v1`). The audit doc records the arming as a
completed improvement; nothing in that pass checked which backends implement the verb.
**REGRESSED-BY-THE-FIX.**

---

### HIGH

---

#### H-1. `GetPruningPointPalwState` runs two uncached full-state materializations per 40-byte p2p request

*(lane: p2p-dos-ibd · **raised by the refuter, judged by one lens only** — the finder listed this
file as read end to end and missed it)*

**Location.** `protocol/flows/src/v8/request_pruning_point_snapshots.rs:135-146` (the bare
`dequeue!` loop), `:150` (`borsh::to_vec` of the full carriage) ·
`consensus/src/pipeline/virtual_processor/processor.rs:13137-13148` (`pruning_point_palw_state` runs
`load_pruning_snapshot` **before** comparing `block == pruning_point` at :13147), `:4581-4585`
(`palw_class_carriages_for_sync_v1_impl` takes an unconditional uncached `load_tip`) ·
`consensus/src/model/stores/palw_state_v2.rs:347-358` (`load_pruning_snapshot` = borsh-decode of the
whole carriage + `into_state_v2` rebuild/walk/root recompute).

**Mechanism.** The flow loops on a bare `dequeue!` with no per-peer budget, no cost accounting and
no rate limit — nothing like the per-`PeerKey` byte budget the same lane has on the material and
interval serve paths — and answers each ~40-byte request with
`session.spawn_blocking(|c| (c.pruning_point_palw_state(pp), c.palw_class_carriages_for_sync_v1()))`.
Both halves are full, uncached PALW-state materializations, and both run *before* anything about
the request is validated: the peer-controlled `pp` is compared to the real pruning point only
after `load_pruning_snapshot` has already decoded and rebuilt the whole carriage. On a hit,
`PalwStateCarriageV2::from_state(&state)` deep-copies the entire state and the flow `borsh::to_vec`s
it, so this is also 40 bytes → multi-MiB outbound amplification.

**Reach.** Any peer that completes the p2p handshake on :16111. No `--profile public-node-rpc`, no
operator opt-in — **every mainnet node**, including non-RPC miners and seats, not just operators who
chose to expose wRPC. Serially per peer and in parallel across peers, on the consensus blocking pool
that block processing shares.

**Mainnet impact.** This is the same M-7 amplification the 09-05 audit already decided was worth
fixing, on a path with strictly wider reach than the RPC methods that pass got. `load_tip_cached`
is right there and covers the class-carriage half; the snapshot half needs the
`block == pruning_point` comparison hoisted above `load_pruning_snapshot` (the tip record's block is
readable without decoding the carriage) plus a per-peer serve budget in the flow loop.

**Refutation history.** One lens only — this was found by the lane refuter and never went through a
second lens or a finder's own guard sweep. The refuter also noted `capture_pruning_point_palw_state`
(processor.rs:13355) is on uncached `load_tip`, and roughly twenty more uncached `load_tip` impls
sit in processor.rs:4401-5640; most are panel/producer-local, but the class-closing fix is to make
`load_tip` private to the fold/restart paths and give every read-side caller the cached loader by
construction, rather than converting call sites one audit at a time.

---

#### H-2. The deferred quality bonus and the reserve drip divide by a denominator from a different epoch numbering, with no pool cap

*(lane: money-mint · finder + refuter (SPLIT) + second lens (SPLIT) — mechanism confirmed, reach and
duration corrected twice)*

**Location.** `consensus/core/src/dns_finality.rs:6871` (`block_epoch = c.block_daa_score / epoch_len`),
`:6876` (`included_by_epoch` filled from `att.epoch`), `:6886` (`anchor_daa = epoch × epoch_len`),
`:3487-3503` (`validator_quality_bonus_outputs` — no pool cap, no count cap), `:3139`
(`proportional_share`), `:6650-6656` (`EpochTally`'s own doc stating the two clocks in plain text) ·
`consensus/src/pipeline/virtual_processor/utxo_validation.rs:1411` (deferred bonus), `:699` (drip),
`:1317` (appended into `validator_reward_outputs`) · `consensus/src/processes/coinbase.rs:319`
(`extend_from_slice`) · `consensus/core/src/config/params.rs:4187`
(`max_validator_inflation_per_block_sompi`).

**Mechanism.** One map keyed by a single integer `epoch`, filled from two numberings.
`quality_pool_accrued` and `expected_stake` are DAA-denominated; `included` is filled from
`att.epoch`, which is blue-score denominated (`attestation_epoch_length_blue_score`,
processor.rs:10605). The payout then divides by that denominator with no aggregate check:
`validator_quality_bonus_outputs` is a bare `filter_map` with neither the `spent + reward > pool`
break (`:3546`) nor the count break (`:3561`) its sibling `validator_participation_reward_outputs`
carries, and `proportional_share`'s `stake.min(expected_stake)` caps each share at the pool, not the
sum. The function's own doc states the safety property as an assumption — "The outputs sum to <=
quality_pool (Σ included stake <= expected_stake)" — and nothing enforces it. Contrast
`victim_compensation_outputs` (`:3805`), which divides by the actual honest-stake sum and is
therefore safe: the codebase knows the right shape and did not apply it here. The φS gate
(`epoch_meets_quality_floor`) fails open in the same direction. Verification is by exact hash, so
an over-mint both paths compute is unanimously valid, and `max_validator_inflation_per_block_sompi`
— the one explicit per-block validator-inflation ceiling in the shipped params table — is consumed
only by the **legacy flat path** (`:3645`, `:3681`) and guards nothing on a card.

**Three regimes, keyed on R = daa_score − blue_score (cumulative reds).** R = 0: numbering
coincides, correct. 0 < R ≲ 603: **over-mint**. R > 603: the paid DAA-epoch indices no longer
intersect the blue-keyed attestation epochs held in the 607-DAA walk, `included` is empty for every
paid epoch, and **every** consumer pays zero forever — the quality bonus, the reserve drip, *and*
`slashed_epoch_victim_outputs` (utxo_validation.rs:621-650), the 40% victim leg of the four-way
slashing split. A chain whose slashing economics are reporter 10 / reserve 40 / victim 40 / burn 10
runs, past that point, as reporter 10 / burn 90 with 40% parked in a reserve that can never drip.

**Price (second lens).** The over-mint is not `k × pool` as a standing state: the only validators
in `included` but not in `expected_stake` are those whose `activation_daa_score` falls in a window
of width R, so on a static bonded set the excess is **zero**. Best case for an attacker, with the
mandated floor of 12 validators (A = 120,000 KAS) and R driven near 601: post C = 1.2M KAS as 10
bonds so every `min(s_i, A)` saturates, extract ≈ 10 × 300 × 800 ≈ 2.4M KAS against 1.2M KAS locked
for one 14-day unbonding period plus ~20 h of ordinary, non-slashable attesting. ~200% on locked
capital with no slashing exposure and nothing an honest node can reject. Real and profitable —
but bounded at (C/A) × (R/2) × ~800 KAS per activation cohort, and dead past R ≈ 601.

**Mainnet impact.** Armed from genesis on a card (`pos_v2_activation_daa_score: 0` in
PRODUCTION_DNS_PARAMS, carried by MAINNET_PARAMS at params.rs:4696), and
`with_two_minute_cadence` rewrites `epoch_length_blocks` to 2, so an epoch is 2 DAA and a threshold
is crossed roughly every block. Either regime means the emission schedule the token's supply story
rests on is not the schedule the chain runs.

**The bigger consequence the second lens found, which is money-mint's, not H-3's:** the missing
**count** cap. The coinbase isolation limit is `mergeset_size_limit + 1 + PALW_V2_COINBASE_EXTRA_OUTPUTS`
= 206, and a crossing block emits one bonus output per included validator with no cap — so ~185
bonded, attesting validators (or ~90 once the reserve funds the drip's second output per validator)
make every crossing block's coinbase fail its own isolation check. With a 2-DAA epoch and monotonic
DAA, the rejected block is re-attempted at the same crossing forever: **a permanent halt in which
the unbond transactions that would shrink the set can no longer be mined.** Cost to trigger
deliberately: ~1.9M KAS. That is the exact 112-block halt the comment at dns_finality.rs:3551 says
the count cap was added to prevent — added to the participation function only.

**Refutation history.** Refuter: mechanism confirmed line by line, reach inverted (the ghostdag-k=1
drift the finder invokes to make it "guaranteed" is what *closes* the mint), and
`max_validator_inflation_per_block_sompi` identified as dead. Second lens: mechanism confirmed
independently, price corrected downward (zero on a static set), the fixture tests confirmed
tautological (dns_finality.rs:11589 has included 100+300 against expected_stake 400, so the `<= 1000`
assertion cannot fail), and the count-cap halt raised as larger than the mint.

**Relation to prior work.** The 09-05 **unjudged** reward/mint bullet names the numbering mismatch
with no consequence attached. New: the consequence is a mint (no pool cap), the reserve drip and the
victim leg share it, the sign flip is permanent, and the tests are fixture-shaped.

---

#### H-3. The widened coinbase output cap leaves one slot at a full mergeset, and the epoch-crossing fan-out is unbounded

*(lane: money-mint · finder + refuter (SPLIT) + second lens (CONFIRMED, and worse than its own
scenario))*

**Location.** `consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs:148`
(`outputs_limit = mergeset_size_limit + 1 + PALW_V2_COINBASE_EXTRA_OUTPUTS`), `:1054-1060` (the
author's own pinned worst case) · `consensus/core/src/palw_state_v2.rs:254, 266, 271` ·
`consensus/src/pipeline/virtual_processor/utxo_validation.rs:1317, 976, 559, 1101` ·
`consensus/src/processes/coinbase.rs:275-291, 334-346` ·
`consensus/core/src/dns_finality.rs:6906-6924` (`epochs_finalized_at`), `:1504-1511`
(`at_two_minute_cadence`), `:3561` (the count cap the sibling has).

**Mechanism.** The cap is 206. ADR-0058 per-red outputs are on for a card
(`palw_pay_entitled_reds_to_their_miner = palw_state_params_v2.is_some()`), so the mergeset half
reaches 180 on its own: 180 + 1 (§D bounty) + 16 (participation) + 8 (escrow) = **205 against 206**,
one slot, before any unbounded kind contributes. The author's own pinned worst case leaves **zero**:
tx_validation_in_isolation.rs:1054-1060 sets `worst_case = 206` and asserts 206 passes and 207 fails.
Then the unbounded kinds append into the same vector, one output per `tally.included` entry **per
finalized epoch** — and with `epoch_length_blocks = 2`, `epochs_finalized_at` returns ~M/2 epochs
for a mergeset of M, so the fan-out is (epochs finalized) × k, not k.

**The threshold, corrected by the second lens.** At mainnet's own `min_active_validators = 12`:
M + 2 + 16 + 8 + k·(M/2) > 206 → **M ≥ 26 halts**. Not the finder's 157 — the finding is ~6× worse
than its own scenario line, because the scenario omitted the multiplier the mechanism paragraph
derived. (The reserve drip contributes 0 until a first slashing funds the reserve; after that,
M ≥ 14.)

**Cost.** `max_block_parents = 10`, so ~9 withheld chain-tips carrying ~28 blocks total. Decisively,
`pick_virtual_parents` (processor.rs:11537-11555) shuffles only candidates beyond
`max_block_parents/2`, and when the candidate count is ≤ `max_block_parents` every candidate is
taken regardless of order — so the randomization that normally gives nodes divergent parent sets
does **not** rescue this: every honest node builds the identical wide mergeset. No bond, no
collateral, no slashing exposure, pure griefing, which is why no economic disincentive applies.

**Consequence.** The template path never checks the coinbase, so the node hands the miner a template
whose `check_coinbase_in_isolation` refuses at body validation. Restart does not cure it: the DAG is
persistent, the virtual is recomputed from the same DAG, and the wide tips can only be merged away
by a block that is itself unbuildable. Permanent halt, curable only by a consensus code change on a
stopped chain.

**Mainnet impact.** Live from genesis on a card. The DNS overlay is Active only at twelve or more
validators, so the network cannot reach its own security floor without arming this. Bounded in time
by H-2's overlap window (the fan-out is non-empty only while cumulative reds stay under ~603) — but
that window *is* the launch period, and a triggered halt freezes the chain so it can never leave it.

**Cost claim, confirmed and under-stated.** `deferred_quality_bonus_outputs`, `reserve_drip_outputs`
and `slashed_epoch_victim_outputs` each perform their own 607-DAA `selected_chain_overlay_window`
walk plus a full `recompute_epoch_tallies`, and each does the walk **before** the `[e_min, e_max]`
filter. With `epoch_length_blocks = 2`, `epochs_finalized_at` returns `Some` on essentially every
block, so a carded mainnet pays two-to-three 607-block chain walks per block on **both** the
template and the validation path. The doc's "O(1) amortized (the deep window walk runs only on the
~1-in-L crossing blocks)" is false on the only preset that arms it.

**Refutation history.** Refuter: arithmetic re-derived and confirmed; two novelty points corrected —
`compute_audit_fee_outputs` is behind the dormant VLT shadow fence (`vlt: VltParams::INERT`,
params.rs:4383) and contributes nothing, and the fan-out is live only in the early-chain window.
`slashed_epoch_victim_outputs` bypassing the coinbase cap into the UTXO diff is correct. Second
lens: confirmed, threshold corrected downward, drip contribution corrected to zero at launch, and
the pinned-worst-case-equals-cap observation added.

**Relation to prior work.** O-15 names the unbounded fan-outs; the arithmetic that pass said was
still owed is here, and the trigger is not the "~1-in-L" event both the audit and the code assume.

---

#### H-4. The seat's free-prompt capture check adjudicates through the **uncapped** refutation walker — the ADR-0084 U-08 site 1cbcb1f6 did not rewire

*(lane: court-adjudicability · finder + refuter + second lens (SPLIT — corrections both ways))*

**Location.** `kaspad/src/palw_panel.rs:1322` (the call inside `fp_capture_samples_clear`, declared
`:1286`), reached from `:2755` · `consensus/core/src/palw_step_refute.rs:4090-4094` (the uncapped
one-line forwarder passing the 2²² default), against the capped sibling three lines below at `:4100`
· `consensus/core/src/palw_step_leg.rs:80, 723-725, 1702-1705` ·
`misaka-palw-base0/src/qwen25_a16_backend.rs:841` (built at `self.step_ladder_cap`) ·
`misaka-palw-sdk/src/lineages/dense.rs:228` (= `court.max_step_leaf_count()` = 2²⁶).

**Mechanism.** Built at 2²⁶, checked at 2²². `step_opening_root_capped_v1` refuses at its first
line with `LeafCountOutOfRange { max: 4194304 }` before a byte of evidence is read; in the panel
that lands in the `Err(other)` redraw arm, so all 32 draws burn and the seat files no `Valid`. The
chain admits what this cannot check **by design**: `validate_v3` compares `work_leaves` against the
*bundle's* `max_step_leaf_count`, with a comment naming the exact sibling defect being fixed ("This
was … the EXECUTOR's 2²² — on a network whose classes are admitted at 2²⁶",
palw_freeprompt_v3.rs:1072-1085). The shape pass one level up already got this right
(palw_step_leg.rs:1580-1600, whose comment says spelling it `PALW_STEP_MAX_LEAVES` "is a CONVICTION
of the honest producer of a class the admission gate accepted").

**Reach.** At 103,008 leaves per prefill position (ADR-0084 §7.2), the threshold is ~40 prompt
positions; capture bytes are decode-dominated (§7.4's 16-token fold = 9,788,323 B, of which
9,723,904 is `16 × 151,936 × 4`), so a 41-position/4-token job is ~2.5 MB and ~4.7M leaves — inside
the 16 MiB transport, over 2²². The class's own canonical job is 6,630,544 leaves. The interval
bypass is dead at every job size: interval 0 carries at least one prefill position = 6.6 MB against
`PALW_INTERVAL_OPENING_MAX_BYTES` = 4 MiB.

**Mainnet impact.** A card FP-certifies the classes it registers from genesis
(`mainnet_certify_registered_classes_v1`), so this is genesis-armed and behind no fence — the panel
reads no fork activation here at all, which is the point: 1cbcb1f6 made the ladder a consensus rule
resolved at the block's DAA and this site kept a compile-time constant that will disagree with it on
every network arming `palw_context_ladder`.

**Refutation history.** Refuter confirmed both ends and narrowed twice: the FPM1 re-execution arm
below (palw_panel.rs:2809-2853) walks no ladder and is reachable for any payload that does not
decode as FPC1, so "no seat can file Valid" holds only on the capture-staged path; and jobs under
~40 prompt positions certify normally. Second lens confirmed and corrected three things — one
decisively **against** the finder: **"it destroys the escrow every time" is false.** `void_claim`
puts the claim on the abandon hold and its own doc states "The hold is a DELAY, never a confiscation
— `release_abandon_hold` gives every sompi back" (palw_state_v2.rs:6141-6164). The real loss is
compute + fee + collateral time + denied weight. And one **for** it: the 32 burned draws are not
cheap redraws — this class's retention is a fold, so `refutation_with_prompt` → `tiles_from_material_v1`
→ `dense_capture_from_fold_v1` **re-executes the whole job** with no cache
(qwen25_a16_backend.rs:798-815, 1325-1333), and `fp_capture_samples_clear` is called *synchronously*
in the panel's verdict task rather than through `offload`, which the FPM1 arm three lines below does
use. One attacker-funded job blocks each drawn seat's whole panel loop for 32 full executions per
pooled payload.

**Relation to prior work.** ADR-0084 §7.1 lists six U-08 sites, all in consensus, all on the close
path; O-1 names those and is marked closed. This site is in `kaspad`, is the **seat's** decision
rather than the court's, and is in neither list. Also new: the audit and the ADR both reason as if a
>2²²-leaf capture cannot reach a seat, from a *decode-token* count — but capture bytes scale with
decode tokens while leaves scale with prefill positions.

---

#### H-5. O-8 is not merely still open — the audit's own fence-arming fix turns it on from genesis

*(lane: prior-open-closure · finder + refuter + second lens · CONFIRMED by both)*

**Location.** `consensus/core/src/config/params.rs:8216-8218` (`if dense_tier_pinned { palw_kary_court
= always() }`) · `consensus/core/src/palw_state_v2.rs:7025-7030, 7148-7154, 7226-7232` ·
`consensus/core/src/palw_bisect.rs:936-939` · `kaspad/src/palw_panel.rs:2306-2495` ·
`consensus/core/src/palw_producer_v2.rs:700-701` (`turn: session.ladder.turn()`; `session.dissection`
never read) · `consensus/core/src/palw_qwen25_profile.rs:1929` (`fused_attention: true` asserted on
the folded genesis class state).

This is C-2's mechanism seen from the closure lane, and it is listed separately because its *novel*
content is about the previous audit rather than about the court: (a) the 09-05 pass's own headline
fix `c4b975bd` is what arms `palw_kary_court` on a card, and the O-8 row does not say so; (b) that
pass's stated grading rule ("a defect behind a fence no card arms is at most MEDIUM") is inverted
here; (c) the rung clock is demonstrably live on a card, because the card takes `PALW_RC_WINDOWS_V1`
whose `court_turn_deadline: 42 < window_court: 3_000`, so `cap_session_rung_deadline_v2`'s
disable-arm does not fire; (d) `palw_court_duties_v2` reports `session.ladder.turn()` and ignores
`session.dissection` entirely — a second half of the missing responder that would misroute even a
future fused responder.

Before `c4b975bd` such a card **could not boot at all**: `palw_v2_params_with_classes_on_base`
refuses a fused graph-v5 row while `palw_kary_court` is `None`, so the failure was loud and total.
After the fix it boots with the court armed and the responder missing, converting a startup panic
into a silent, per-claim collateral drain on the tier carrying 489‰ of the cadence.
**REGRESSED-BY-THE-FIX.**

**Second-lens pricing:** ~1.72M sompi (0.017 MSK) of *returned* collateral funds all 42 global
session slots; the victim loses `claim.reserved` **plus** the escrowed reward, because
`palw_reward_status_v2` maps every `Voided` to `Forfeited` (palw_reward_v2.rs:92, pinned by
`the_escrow_ladder_is_total_over_the_lattice`: "every void reason forfeits — no reason is a
discount"). At `WORKER_CARVE_PERMILLE = 620` of the year-1 subsidy that is ~2.30e9 sompi of reward
destroyed per conviction against 20,480 sompi of collateral — a griefing ratio of ~56,000:1.

**Refutation history.** The refuter established that the finder's `class_holds_weight` argument is
decorative: the round-0 mercy requires `session.ladder.round() == 0`, and a converged ladder has
`round() > 0` by construction, so the mercy **cannot apply at the fused terminal for any class,
weight-bearing or not** — the defect is broader than claimed and the finder's derivation was not the
load-bearing one. It also corrected the rate downward (a graph-v5 session needs ~54 rungs). The
second lens then corrected the refuter's throttle: `palw_court_move_spends_the_slot_v1` matches only
the three dissection objects, so `PALW_COURT_CLOSE_MAX_PER_BLOCK = 1` does not serialize the ladder
and all 42 sessions run in parallel — the dense tier's revenue goes to zero, it is not bled.

---

### MEDIUM

Fourteen findings. Six of them (M-9 … M-13, M-16) are behind `palw_model_market` /
`palw_model_lines` / `palw_model_evm`, which no card arms — MEDIUM by that rule alone, and each is
a launch blocker *for the day those fences are armed*, which ADR-0087 D6 says will be by activation
on a live chain.

---

**M-1. The security-reserve drip's per-epoch cap is applied to a 2-DAA epoch.**
`utxo_validation.rs:663-706` (`budget = remaining.min(cap)` **inside** `for (epoch, tally) in &tallies`)
· `dns_finality.rs:1505` (`at_two_minute_cadence` rewrites `epoch_length_blocks` 100 → 2) ·
`params.rs:4234` region (`reserve_drip_per_epoch_cap_sompi: 1000 × SOMPI_PER_KASPA`). The cap is
documented as a rate limiter and the rate it delivers is `cap / epoch_duration`; the epoch was
silently redefined by a rescale that names only `reward_uniqueness_window_blocks` as its deliberate
omission. `epochs_finalized_at` returns ~mergeset/2 epochs, so a wide-mergeset block drips up to
~90 times and the only stop is `remaining == 0`. Value-conserving (not a mint) — the harm is that
the four-way slashing split's reserve leg does not function as a reserve. Armed from genesis on a
card, but needs a slash to have happened first. *Refuter's precondition:* the drip mints nothing
unless `tally.included` is non-empty, so this is the same two-regime shape as H-2 — early life, the
reserve empties inside a single block; after R ≈ 603, it never drips at all. Not on any prior list.
Single lens.

**M-2. At every epoch boundary the chain block cannot be a non-floor attempt block, and every
non-floor attempt that boundary block merges loses its claim permanently.**
`palw_admission_v2.rs:357-362` (`epoch_index` from the **child's** `ctx.daa_score`, table read off
the **parent's** `state`) · `palw_state_v2.rs:6591` (`ensure_epoch_budgets` runs at step 3b, so the
fold is strictly *more* permissive than its own pre-check) · `processor.rs:1481-1497, 1553-1563,
7046-7057`. The floor class, the heartbeat lane and the receipt lane are exempt, so this is
liveness and fairness rather than a halt. A block is merged by exactly one chain block and a skipped
work is never retried, so the claim, its escrow and (per the refuter, via processor.rs:4948) the
worker carve are gone. The refuter added that the boundary block itself is never merged either,
because `sink_search_algorithm` discards it rather than returning it as a virtual-parent candidate —
permanently orphaned, not deferred. Behind no fence; unconditional on a `ConsensusV2` network.
Once per epoch (~33 h at `EPOCH_LENGTH = 1_000`), or attacker-timed by anyone who can push the DAA
score. This is the 09-05 **unjudged** time/DAA item, established from code here. Single lens.

**M-3. A free-prompt claim's DA obligation is three numbers its own producer writes, and one of
them is not carried at all.** *(second lens lowered this from HIGH)*
`palw_state_v2.rs:9743-9744` (both fields stored verbatim), `:9445-9467` (the accusation's only
gates), `:2244-2256` (the doc claiming the pin) · `palw_admission_v2.rs:534-567`
(`check_palw_attempt_da_pins_v1` — ADR-0072 D8 made real, whose only caller is the **attempt** lane
at `:632`/`:687`) · `palw_freeprompt_v3.rs:1087-1089` (the only free-prompt check on either field) ·
`palw_fp_objects_v3.rs:180-196` (extraction drops `trace_manifest_root`). Writing
`trace_retention_daa = 0` makes every `DefaultAccused` of that claim refuse `DaOutsideRetention`
forever, at zero cost, and nothing on the licensing path reads the field. On a card
`palw_unavailable_abstains` retires `ProducerUnavailable`, so the DA court is the **only**
withholding-to-slash path — and it is closable for free by the party it exists to prosecute.
*Refuter:* the k-ary court is not closed by this; `trace_retention_daa` appears in `palw_court_v2.rs`
only as a test literal. *Second lens (demotion):* the immunity is worth at most
`claim.reserved <= collateral × 0.5` over a 4,200-DAA window, on a court whose shipped seat never
accuses automatically by design (palw_panel.rs:2530-2532); a successful disclosure *slashes the
accuser* (`:9593`), so the DA court was never a discovery route for C-3's un-opened rows, with or
without this defect; and `trace_chunk_count = 1` is the *honest* value for any run ≤ 256 tokens
(`PALW_FP_TRACE_CHUNK_EVENTS_V3 = 256`), so that half is inert at shipped budgets. The argument for
fixing it before genesis is different and stronger than the finder's: **writing 0 is weakly dominant
for an honest producer too**, because it also immunizes against griefing accusations that pause the
claim — so the field will drift to 0 network-wide with no adversary at all. Related to the 09-05
unjudged "a claim's DA window is a number its own producer writes".

**M-4. A chain-legal `EndOfGeneration` claim is excluded from the ADR-0077 D8 interval lane.**
`qwen25_a16_backend.rs:1512-1520` and `:1183-1191`, `qwen36_backend.rs:1152, 1464`,
`backend.rs:338, 651, 1249` (all six sites hard-pair `decode_tokens_executed = job.decode_token_limit`
with `stop_reason: ExactBudgetReached`) · `palw_panel.rs:4118-4119, 2661` ·
`fp_interval.rs:286-296, 334-340` · `palw_freeprompt_v3.rs:1074-1081, 410-415` ("EOG is a legitimate
stop for a user answer (Decision 7)"). The seat's interval count is `f(limit)` while the capture has
`f(executed)` intervals, so surplus indices are unopenable by construction (`calls_for` returns
`None`). Because `execute_free_prompt` builds its pre-run facts the same way, the shipped worker
**never emits the stop reason at all** — the D7 user path has never once run, and every free-prompt
answer is padded to the declared budget. *Refuter (SPLIT):* `interval_seat_outcome_v1` returning
`None` does not end the seat's round; control falls through in the same iteration into the
whole-capture arms, which can still `break 'verdict Some(Valid)`. Corrected claim: a chain-legal
EOG claim is forced back onto the whole-capture path D8 exists to replace — a liveness/scalability
regression, hard only where the capture cannot be served (the ADR-0084 material cap). The 09-05
unjudged list names the seat's rebuilt context; the failure direction, the six sites, and the
`palw_unavailable_abstains` interaction (on a card the outcome is silent exclusion rather than the
honest producer being slashed, as on testnet-11 today) are new. Single lens.

**M-5. `GetPalwProducerFacts` runs an uncached full-state materialization per unauthenticated wRPC
call — M-7's fix skipped this method and its locked-bond helper.**
`rpc/service/src/service.rs:967-970` (unconditional, **before** the empty-class-id branch at `:977`),
`:998` · `processor.rs:4778, 4700` (both on uncached `load_tip`) · contrast the fixed paths at
`consensus/src/consensus/mod.rs:1254, 1272` (`load_tip_cached`) · the misdescribing comment at
`components/consensusmanager/src/session.rs:369-370` ("a lock-free snapshot read"), which is how the
gap stayed invisible. `load_tip` = `materialize_tip` = borsh-decode of the whole carriage +
`rebuild_deadline_free_indices` + `rebuild_deadline_index_v2` + two consistency walks + a blake2b
`state_root()`. *Refuter's under-claims, both in the finder's favour:* the handler is an `async fn`
calling the sync session methods with no `spawn_blocking`, so it starves the RPC runtime's async
executor rather than a blocking pool; and this is not the last surviving M-7 instance — see H-1,
which is the same defect on the p2p port with no operator opt-in. Node-local liveness, no consensus
effect; the public wRPC listener is loopback unless the operator opts in. Single lens.

**M-6. On a carded mainnet the fork-id handshake gate is structurally inert for every fence a card
leaves dormant.** `consensus/core/src/fork_id_v1.rs:225-236` (`fork_id_gate_fences_v1` names exactly
three fields and filters `score == 0`), `:195`, `:273-279` (`Unfenced` returns before the
`armed_fired_through()` fallback) · armed at `params.rs:8228-8231` · consumed at
`protocol/flows/src/flow_context.rs:1731-1749` (`Unfenced` → peer kept). A card arms both ADR-0083
halves at `always()` (score 0, filtered out) and never writes `palw_attempt_activation`, so
`fork_id_gate_armed_v1(carded) == false` — permanently, and it stays false when the operator later
schedules any of the other eighteen fences, none of which is in the list. The card's own test reads
the emptiness as a virtue ("nothing scheduled: everything is at genesis", params.rs:12592). The
sibling guards do not substitute: `consensus_identity_id` normalises a scheduled fence to
`never()` → `None` *deliberately*, so a scheduled fork does not partition the mesh, and the
`consensus_params_id` mismatch is a `warn!`, not a gate. **This falsifies the 09-05 audit's own
closing sentence** — "the fork-id gate lists the scheduled heights so an un-upgraded peer is refused
at the handshake rather than at consensus" — which happens to hold on testnet-11 (whose
`palw_difficulty_priced_rows = Some(1150)` *is* a gate fence, so the `armed_fired_through` fallback
reaches `DisagreePastFence`) and is false on the carded mainnet it was written about. *Refuter
(SPLIT):* not "structurally dead" — the list is extensible and arms normally once a fence is added
to it; and there is no attacker path today. Corrected claim: a process/guarantee gap on the card
path, and the required per-fence edit to `fork_id_gate_fences_v1` (which the module's own doc
prescribes at `:214`) is missing from the card checklist.

**M-7. A carded mainnet cannot be assembled at all, and the tests that claim to exercise the card
patch the field that blocks it.** `params.rs:8130-8187` (`mainnet_shipped_params`, ending in
`.unwrap_or_else(|e| panic!("the pinned mainnet PALW genesis card does not assemble: {e}"))` at
`:8186`) · the gate at `:1628-1637` · `palw_v2_params_on_base` at `:8628-8725`, which imposes
cadence and depths and never touches `genesis` · `genesis.rs:167` (`bits: 0x1f7fffff` against
`MAX_DIFFICULTY_TARGET`'s `0x207fffff`) · the test-only substitute at `:12176-12180`
(`mainnet_v2_mint_base`, which performs exactly the omitted write) and its nine users. The only
production writes of `genesis.bits` are `with_palw_v2_cadence` (`:1469`, called only from
`virtual_processor/tests.rs`) and the testnet-11 mint tool. `Params::from(Mainnet)` routes to
`mainnet_shipped_params`, so a carded binary panics at node start. Fail-closed, so no live chain is
at risk — but the gate compares `genesis.bits` against `max_difficulty_target` and **either side**
satisfies it, and lowering `MAINNET_PARAMS.max_difficulty_target` is the smaller-looking edit that
does not move the genesis hash. That would reinstate the 256× wedge the same audit's item 3 removed.
The one-line fix already exists in the tree (`palw_v2_params_on_base` performs the two halves of
`with_palw_v2_cadence` without its mint). *Refuter's correction to the framing:* the runbook's named
verification command does **not** panic — `shipped_presets_have_pinned_fingerprints` pins
`MAINNET_PARAMS` (`:11412`), the bundle-free constant, so it stays green with a filled card and
never exercises the assembly at all. **Mainnet has no green-able assertion over
`mainnet_shipped_params()`.**

**M-8. `signature_contexts_root` commits 9 contexts while the acceptance layer verifies at least 17,
and the test that closes it is a copy of the same 9.** `palw_mode_v2.rs:107-121, 124-136`, test at
`:1810-1826` · uncommitted contexts with live production verify sites: `processor.rs:3083`
(carriage commitment), `:6406` (bond capability), `:6439` (DA accusation), `:6485` (DA disclosure),
`palw_court_v2.rs:650, 524, 547, 571` (close declaration, attn responder, attn challenger),
`palw_freeprompt_v3.rs:1107` (FP commitment). The field exists so "a build whose contexts differ
from what the network committed to refuses to run", because "a refused object is SKIPPED and the
block stands, so such a disagreement splits the class registry with no block ever rejected and
nothing in either log saying so". The M2-23 repair is incomplete **on its own terms** — its comment
names the carriage commitment, the array does not contain it — and three contexts added after it
were never added. `validate_ruleset_shape` pins the field to *this binary's* recompute, which is
exactly why an incomplete list is invisible to it. On a card the DA and court contexts are
consensus-live from block one. Not attacker-triggerable on its own: it needs a build whose context
spelling differs while every other borsh-committed ruleset field is identical, which is realistic
only for an in-place edit or a deliberately modified binary. New; no prior audit mentions
`PALW_V2_SIGNATURE_CONTEXTS`. Single lens.

**M-9. A build that merely *schedules* `palw_model_market` relaxes a consensus transaction rule from
genesis, and the handshake deliberately keeps it peered with one that has not.** *(MARKET)*
`consensus/src/consensus/services.rs:158` (`params.palw_model_market.is_some()` — the presence of the
Option, never its height) · `tx_validation_in_isolation.rs:100-106` (context-free; no DAA in scope) ·
`params.rs:2941-2947, 2240-2242` (identity normalises a scheduled fence to `never()` then to `None`)
· `flow_context.rs:1675-1696` (identity agrees → peer kept, warning only) ·
`body_validation_in_isolation.rs:117-124` (a tx that fails isolation invalidates the block). The
consumer's own doc claims the opposite of what it does. This is the only `is_some()` read of the
fence in the tree; every other consumer uses `palw_model_market_active_at`. It also falsifies the
load-bearing safety comment at `kaspad/src/args.rs:608-611` that the devnet drill's isolation
premise rests on. *Refuter (SPLIT):* the disagreement is one-directional (the upgraded side accepts
both arms), so it is an attacker-timed *premature flag day* for the lagging side, not a symmetric
permanent fork; and no card declares the fence. Same shape, ungated on every network:
`mining/src/mempool/check_transaction_standard.rs:159-166, 187-192`, held up today only by the
consensus isolation rule running first — so the fix M-9 requires (moving the rule to a
context-bearing check) must move those two carve-outs with it.

**M-10. The model market is a second, unbounded writer into the 8-per-block payout queue whose
sizing argument is "at most one new claim per block".** *(MARKET)*
`palw_state_v2.rs:245-254` (the constant and its verbatim premise), `:6325-6345` (`write_model_fee`
→ `write_payout`), `:10733-10736, 10912` (up to three legs per sell), `:7240` (the drain,
`take(8)`), `:10833/10872/10917` (`model_moves` is a key nonce, never a cap) · `processor.rs:5086-5098`
· `palw_economic_locus_v1.rs:48, 77-84`, which states the premise, names three places that rest on
it, and warns "A design that turns one answer into N claims negates the premise all three rest on,
and owes each of them an argument" — and was not updated when ADR-0087 landed. No per-block object
cap applies: the rehearsal's three counters key on `FamilyCertified`, `CourtCloseDeclared` and court
closes. `pending_payouts` is in the state-root preimage, and **claim rewards live in the same map**,
interleaved in hash order. At ~15 KB per sell against a 500 KB block, ~33 sells/block writing ~66
rows against a drain of 8. *Refuter:* row count corrected from ~99 to ~66 (a leg with amount 0 or a
payee with no bond writes nothing), and the attacker's own proceeds queue behind their own spam.
Note the coupling the finder missed: the 8-per-block drain is simultaneously the only bound on
M-12's unfunded mint rate, so whoever fixes either must fix both. Single lens.

**M-11. A `ModelSell` signature is a permanent bearer authorisation.** *(MARKET)*
`palw_model_market_v1.rs:229-238` (the signed message: domain, class, holder, units, floor — no
nonce, no carrier, no network domain) · `palw_state_v2.rs:2909-2917, 10877-10918` (the only state
guard is `units_in <= held`) · `palw_lifecycle_objects_v2.rs:371-400` (the carrier binder returns
`Ok(())` for anything that is not a Buy or a Seed) · `palw_lifecycle_objects_v2.rs:433-467` (the
extraction walk does no payload deduplication) · `misaka-cli/src/palw_model.rs:390`
(`min_msk_out` defaults to 0). Every ADR-0088 registry object signs over `palw_network_domain_v2()`;
this one does not. A stranger copies the public payload into a new transaction and re-fires it
whenever the holder holds units again. Not theft of principal — the net leg is paid to the holder —
but a forced-liquidation and MEV primitive with no revocation and no expiry, and the owner collects
1% on every re-fired leg. *Refuter:* the cross-network half is conditional on the same line id
existing on both chains, and the victim is force-exited at a price they consented to once (losing
the 6% round trip and the position, not principal). Fixing the message later is itself a hard fork
of the object. Single lens.

**M-12. Every model-market payout is minted into the coinbase with nothing withheld.** *(MARKET)*
`processor.rs:4898-4907` (`palw_v2_escrow_withheld_at` sums only `claim.escrowed_reward`, under a
doc that says "the number withheld is the number that will be paid — by construction, from the same
record"), `:5086-5098` · `utxo_validation.rs:1004` · `coinbase.rs:133-137, 245` ·
`palw_state_v2.rs:6325-6345, 10912` · `palw_model_market_v1.rs:180-192` (the sink is a normal,
permanent UTXO) · `docs/adr/0087-*.md:43-44, 153-156`. A market payout has no claim and no
`escrowed_reward`, so it is withheld from nothing, and the coinbase appends the queue prefix
verbatim. Verification is by exact hash, so both sides mint identically and nothing rejects it. The
ADR's own invariant M2 is tested as an identity over the market **row**
(`paid_in + seed == reserve + net + burned + registrant`) — a bookkeeping tautology true no matter
what the coinbase does. *Refuter's important correction:* the headline "the 5% burn destroys no
coin" is half wrong and must not ship as stated — the burned fraction is never paid out of the sink,
so it is permanently unspendable, exactly as ADR-0087 §2 promises; what is not true is that it
leaves the UTXO set. And because Σ payouts ≤ Σ sunk, **spendable** supply never exceeds premine +
emission, so ADR-0059's 10 B cap is not breached. What survives: ADR-0042 D10's "a carve, never an
addition" is false for the market lane, and any supply readout that sums the UTXO set — including
this repo's own `indexes/utxoindex/src/update_container.rs:33`, which adds every new UTXO with no
spendability test — over-reports by the total ever sunk. `write_model_fee` also "burns" a leg by
simply returning the amount when the payee has no bond (`:6331`) while incrementing
`after.burned_sompi` at `:10864`. Single lens.

**M-13. `model_positions` is an unbounded, attacker-keyed consensus table, deep-cloned per chain
block and per `eth_call`.** *(MARKET)*
`palw_state_v2.rs:2897-2905` (`ModelBuy { holder: Hash64 }`, checked only against
`Hash64::default()` at `:10842`), `:10870-10871`, `:4309-4312, 5044-5047` (both tables in the root
preimage), `:4763-4885` (`evm_view_v1`, ending `positions: self.model_positions.clone()`) ·
`consensus/src/consensus/mod.rs:1358-1375` (`palw_evm_view_v1` checks only that the mode is
ConsensusV2 — it builds the whole view and returns the fences *alongside* it, never consulting them)
· `kaspad/src/eth_rpc.rs:832-838` · `processor.rs:1824-1827, 2322-2327`. The holder is a free field
authorised by the sink output rather than by a signature, so each buy to a fresh holder id creates a
row only that key's owner could ever remove. *Refuter:* the DoS framing is over-sold — the marginal
cost is a ~1.3 MB `BTreeMap` copy at the finder's own 10,000 rows, sub-millisecond against an
`eth_call` that then runs a whole EVM simulation, and each row costs a whole abandoned position on a
curve whose marginal unit is strictly more expensive as the table grows. The defensible pair:
`model_positions` is an unpriced, never-swept, attacker-keyed consensus table with no per-line holder
cap, **and** `evm_view_v1`'s cost is linear in it and is paid per `eth_call` with no fence and no
per-caller budget. The per-chain-block build (processor.rs:1826) is the more interesting consumer
than `eth_call`, because it is on the consensus hot path. Note the RPC half is only partly fenced:
`palw_evm_view_v1` runs today on any ConsensusV2 network including a carded mainnet, and is cheap
only because the tables are empty. Single lens.

**M-14. O-12 is marked closed and its fix sits behind a fence no preset arms and no card states.**
`palw_state_v2.rs:8235-8237` (`if builder.capability_bound { check_capable_classes_declared_v1(...) }`
— the whole of the fix) · `params.rs:8215-8243` (the card states eight fences; not this one),
`:8855` (`MAINNET_PARAMS.palw_capability_bound: None`), `:10166-10175` (a test **requires** every
preset *and* `palw_rc_shipped_params()` to leave it dormant, so the gap cannot close by accident).
The set-difference test the same pass wrote to catch exactly this class of omission is structurally
blind to it: it computes `rc_armed \ mainnet_armed`, and a fence dormant on both sides never appears
in either. With the fence off, `PALW_MAX_CAPABLE_CLASSES = 16` and the SA-2 exposure price are both
unreachable at registration. The 09-05 pass's framing sentence — "Everything that touches a live
chain's validity sits behind a fence testnet-11 leaves dormant **and a card states from genesis**" —
is false for this fence. *Refuter (SPLIT, downgraded from HIGH):* drop the panel-eligibility payoff
— it is reachable through `BondCapabilityDeclared`, which the O-12 commit never touched and which is
equally unbounded for *registered* ids under the same dormant fence, so closing the registration arm
would not have removed it; and drop "permanently in `state_root`", since retirement clears the set
(`:8309`). What survives is a mass-priced, per-bond-lifetime state-growth lever plus the SA-2 price
never being charged — a "closed" row whose commit is inert on every shipped configuration.

**M-15 (bookkeeping).** Two branches close ADR-0084 U-08 with two different fence fields at the same
three call sites, and no test binds either field to the processor that reads it. COURT:
`params.rs:1113-1140` (`palw_context_ladder`), `:8218` (the card arms it), `processor.rs:6726,
6739-6746`, call sites `:4425, 5876, 5940, 6213`. MARKET: `params.rs:1033-1044`
(`palw_court_ladder`), `:2423-2434`, `processor.rs:6949-6955`, call sites `:4529, 5954, 6211`. The
off-tree three-way merge from `32c772cc` produces **one** conflict in params.rs (an unrelated
accessor block) and **seven** in processor.rs, all at exactly these hunks — so a human resolution
decides which field a carded mainnet's arming table has to name, and nothing observes the choice:
`the_refutation_ladder_is_the_rulesets_only_past_the_context_ladder` (params.rs:12336-12353) computes
the fence boolean itself and calls the consensus-core helper directly, never touching the processor;
`git grep palw_refutation_leaf_cap` shows the processor's three call sites have **no test at all**.
*Refuter (SPLIT):* the headline "a card arming a fence that gates nothing" describes a state that
exists in **neither** tree — MARKET contains no `mainnet_card_base_v1` and no `palw_v2_fence_table`,
and on MARKET `palw_court_ladder` is `None` on every preset. Corrected: a merge hazard plus a test
hole, not a live card-path defect. Recorded here because the merge is planned.

**M-16. The EVM market's 128-actions-per-block budget is first-come and a refused sell costs only
gas.** *(MARKET)* — see L-6; the refuter downgraded it to LOW.

---

### LOW

**L-1. `past_median_time_window_size` is the third cadence-derived quantity `with_two_minute_cadence`
leaves behind.** `config/constants.rs:23, 26, 29-30` (27 samples sized as 263 s) ·
`params.rs:256-257` (`past_median_time_sample_rate: 1`), `:8712-8717` (the 09-05 fix block, which
moves three quantities and not this), `:4639, 4776, 4937, 8765` · `past_median_time.rs:18-39` ·
`post_pow_validation.rs:21-30`. At 120 s a block the window spans 54 minutes against the 263 s its
own constant models — a 12.3× overshoot, and `TESTNET_PARAMS` carries a hand-written comment
explaining the analogous derivation for `difficulty_window_size` two lines below the untouched
sibling. **The refuter downgraded this from MEDIUM and rejected the prescription as actively
harmful:** `AVERAGE_FRAME_SIZE` is 11, so the finder's "correct value is about 3 samples" makes the
MTP the plain median of 3 timestamps, which any producer holding two of three recent blocks sets
outright. The mis-sized quantity is `timestamp_deviation_tolerance`, not the window. Observable
effect: a ~26 min backdating band (Bitcoin-like), a ≤5% one-directional retarget discount, a one-off
~13-block heartbeat DAA burst that must be repaid in wall clock, and a ~26 min lag on lock-time
finality. Worth an assertion relating
`past_median_time_window_size × sample_rate × target_time_per_block` to
`2 × timestamp_deviation_tolerance` before a mint — `params.rs` has none. New; the 09-05 item 2
states its own scope as two quantities and there are three, and unlike `difficulty_window_size` this
one is wrong on testnet-11 and devnet too.

**L-2. `max_prompt_tokens` and `max_decode_tokens` are ruleset fields with zero readers.**
`palw_freeprompt_v3.rs:720-724, 757-762, 789-795` · `palw_fp_devnet_v3.rs:414-415` (512 / 1,024)
and `:789-790`. Both are validated at construction and hashed into `palw_ruleset_id_v2`, and a
repo-wide grep returns only the declarations, the constructor's range checks, the field writes and
the accessor bodies. `validate_v3` never takes the params struct, so the caps are structurally
unreachable from the only validation the lane runs; the surviving bounds are transaction mass and
`work_leaves <= max_step_leaf_count`. A node advertises caps in its fingerprint that it does not
enforce. *Refuter (downgraded from MEDIUM):* the escalation to C-3 is false — the crafted context
there is (prefill 1, exact_decode 512), comfortably inside both stated caps — and there is no
divergence, since the field is unread on every node identically. A dead ruleset field advertised
inside the fingerprint, repairable only at genesis or by activation. Not on any prior list.

**L-3. The set-equality fence table is the one fence list the compiler does not police.**
`params.rs:12183-12211` (`palw_v2_fence_table`, a hand-written `vec![]` of 21 entries) and
`:12227-12303` · the fences it will not see: MARKET `params.rs:1044, 1049, 1055, 1062`. Every other
place a fence must be listed is compiler-forced by exhaustive destructuring (`for_each_fence`,
`consensus_params_id`, the preset literals); this one is not, and the market branch's four new fences
join `Params` without joining it, with zero merge conflict at the table. The audit's own summary
claims the property as achieved: "the next fence added cannot be armed on one shipped network and
forgotten on another." *Refuter (SPLIT, downgraded):* nothing is mis-armed on either tree today
(all four are `None` on every preset) and the exposure needs a merge plus a later arming asymmetry —
erosion of a test-only guard, not a ruleset defect. Cheap fix: derive the table from
`for_each_fence`, or assert its length against the exhaustive destructure.

**L-4. `docs/adr/README.md`'s activation-axis table is stale in both directions.**
`docs/adr/README.md:174-188` against `params.rs:8489-8520, 8215-8236, 8273-8340`. The testnet-11 row
lists three fences; `palw_rc_base_params` arms six (adding `palw_uncertified_weightless`,
`palw_kary_court` and `palw_difficulty_priced_rows` at 1150). The mainnet row says "when set, the
same arming as the RC applies from genesis"; a card now states up to thirteen. The table's own
preamble claims it is "read from the assembly functions … rather than from any ADR's Status line",
which is the property it no longer has. `docs/mainnet-palw-certification-runbook.md:14-15` likewise
omits the ambient-target mint of M-7 (grep for "bits"/"ambient": no hits). Documentation only — a
mis-specified card is refused at assembly rather than minted — but this lane's question is whether a
mint produces the ruleset the docs claim, and it produces a strictly larger one. The 09-05 pass
quoted this document as the specification a card failed to meet and then fixed the code without
updating the sentence, so the same sentence is now wrong in the opposite direction.

**L-5. The remote signer's `RESERVED_CONTEXTS` names 6 contexts while `SigningPurpose::Transaction`
accepts a caller-chosen 64-byte digest under any other.** `kaspa-pq-signer/src/lib.rs:463-504,
489, 577-586` · request shape at `dns_finality.rs:2947-2962, 2812-2814`. The comment at `:487-488`
claims the derived-artifact context is reserved; the array above it does not contain it. Bond
retirement, capability, registration, class registration, DA accusation, DA disclosure and receipt
are all verified over a `Hash64` message with their own context, so a `Transaction` request mints
them bit-identically. `--deny-purpose` — widened by the 09-05 pass so "a signer that must never
spend could not say so" — is bypassed by re-asking as `Transaction`, and denying `Transaction` is
not an option for a signer that must sign spends. **The refuter downgraded this from MEDIUM on
reach:** grep finds no client of this protocol anywhere in the tree; the two `UnixStream` clients
speak the agent protocol. A latent hardening bug in an unwired component — MEDIUM the day a signer
client lands. Fix by inverting the model so `Transaction` may carry *only* a tx-domain context.

**L-6. The EVM market's 128-action budget is first-come and a refused sell costs only gas.**
*(MARKET)* `consensus/core/src/evm/model_market.rs:66-70` · `kaspa-evm/src/model_market.rs:1302-1312,
1329-1336, 1354` (a Sell is accepted on value 0 with only `units == 0` refused; there is no position
check, because ADR-0089 D6 makes the fold's refusal a settlement rather than a fault) ·
`palw_state_v2.rs:10891`. **The refuter killed the load-bearing claim:** "no per-account share, no
fee-priority ordering, no reservation for honest flow" is false — `mining/src/evm_mempool.rs:643-666`
selects on `effective_tip(base_fee)` and `:498-512` carries per-sender caps
(`EVM_MEMPOOL_MAX_TXS_PER_SENDER = 256`, a declared-gas cap), so filling the budget is an ordinary
fee auction at parity with honest users. *The residual worth writing down, which neither the finder
nor the mempool covers:* `processor.rs:1855-1858` sources the executed set from
`consensus_ordered_mergeset` (selected parent first, then ascending blue work), so ordering **across
payload blocks** is consensus-determined, not fee-determined — an attacker who mines, or who buys
placement in the selected parent's payload, occupies the budget without outbidding. The finder's
coverage statement never listed `evm_mempool.rs`; that omission is the one real gap in the
model-market lane's coverage.

---

## Prior-audit closure

The `prior-open-closure` lane re-verified the 2026-09-05 follow-through table item by item, reading
all 15 named commits' messages and the full diffs of `3d705bbc` and `c4b975bd`. Its findings are
new information about that pass, not a restatement of it.

### Rows that hold as the table says

**O-1 closed and armed on a card.** `palw_refutation_leaf_cap_at` is resolved once at
`processor.rs:6739` and threaded to `open_against`, `open_checkpoint`, `verify_kv_anchor`,
`check_tiled_decode_token_refutation_v2` and `check_execution_step_refutation_opened_capped_v1`; the
remaining `step_merkle_root_v1` hits at `palw_step_refute.rs:2608/2626/2637` are prover-side root
builders over real data, not verifier caps. The court lane independently enumerated every remaining
caller of the uncapped names and found only `palw_carriage.rs:1066`, reached from
`palw_facts.rs:545` and `processor.rs:13711`, both of which run with `PalwNoWeightsV1` and derive
nothing at any ladder. **But see H-4:** the sweep covered `consensus/` and not `kaspad/`, and the
capped and uncapped functions are three lines apart differing only by a suffix. Every uncapped
`*_v1` walker name should be a lint target across the whole workspace.

**O-14 closed on a card** (`palw_fp_certified_class_ids_of_registered_v1` filters by
`registered.contains(&profile.shape_profile_id())`, with the id/profile-id identity pinned by test).
**O-16 closed** (`ibd/flow.rs:2570-2585`; the carriage is root-checked in
`import_pruning_point_palw_state` before any declaration is spent, and the import runs first).
**O-17 closed as stated**, residual: a solicited claim is exempt and duplicates still spend budget —
bounded, not new. **O-18 closed at every site** (`palw_backend.rs:71` used at `palw_panel.rs:88,
2733, 2842` and `fp_interval.rs:1611, 1882`); the relaxation to "no check when `work_leaves == 0`"
is safe on the replay route because root equality already reproduces the class's canonical work.
**O-4 and O-7 (retarget half) closed and armed on a card. O-2 bounded as stated.**

The **"should fix"** item `palw_state_v2.rs:8735` **is not a defect in this tree**: `index >= count`
(`:8757`), the duplicate refusal (`:8770`) and completion at `parts.len() == count` make the index
set exactly `0..count-1` by pigeonhole, so `pending.parts[&i]` cannot panic. The p2p lane reached the
same conclusion from the other end: the only import path recomputes `state_root()` and demands it
equal the witness child's committed root, so a root-matching carriage cannot carry the malformed
shape.

**Also cleared by reading** (silence here is a clearance): the court's ladder *does* fit its window —
`PALW_RC_WINDOWS_V1` already moved `court_turn_deadline` 60 → 42 for exactly this (54·42 + 216 =
2,484 < 3,000; 66·42 + 216 = 2,988 armed), and `PalwConsensusParamsV2::validate` refuses a bundle
that does not fit. The module doc at `palw_court_deadline.rs:34-44` is stale ("2^22 ladder … the RC
ships sixty") — documentation only. "A guilty responder escapes by disclosing junk at every rung" is
refuted: only the challenger's `agree` bit moves the interval. The DA court's disclosure arithmetic
is sound (the constant at palw_state_v2.rs:9575 is genuinely sufficient; the 151,936-vocab flat
profile cannot pass the cost gate and is not registered). The 10 B premine cap holds arithmetically
on a card, the Decision-10 escrow is conserved, and only the selected parent's coinbase enters the
UTXO set.

### Rows that do not hold

| item | 09-05 status | 2026-09-06 finding |
|---|---|---|
| **O-12** | **closed** | **half-closed** — the fix is behind `palw_capability_bound`, which no preset arms, no card states, and a test *requires* to stay dormant. M-14. |
| **O-8** | open, "the missing half is the responder's software, a feature rather than a patch" | **regressed by the fix** — `c4b975bd` arms `palw_kary_court` on a card, so the passive gap is now live from genesis; and it is not a liveness gap but a steered, evidence-free, funded conviction. C-2, H-5. |
| *(no row)* | — | **created by the 2026-09-06 follow-through** — `f7363db9` arms `palw_da_court` over two families with no responder. C-5. |
| **O-15** | "open in the tail" | the widened cap leaves **one slot** at a full mergeset (the author's own pinned worst case leaves zero), and the halt threshold at the mandated 12 validators is a mergeset of **26**, not 157. H-3. |
| **O-7** | "closed, fenced" (retarget half) | the receipt lane still pays no PoW and its blocks still advance the DAA score; the fix addressed the retarget-window half only. See `unverifiable`. |
| — | follow-through prose: "the fork-id gate lists the scheduled heights so an un-upgraded peer is refused at the handshake" | **false on a card** for all eight fences a card leaves dormant. M-6. |
| — | follow-through prose: "Everything that touches a live chain's validity sits behind a fence testnet-11 leaves dormant and a card states from genesis" | false for `palw_capability_bound`. M-14. |
| — | "Armed only on a carded mainnet: `palw_da_court`, `palw_prompt_ids_merkle`, `palw_panel_da`. testnet-11 and devnet do not move." | **incomplete**: arming `palw_da_court` on a card also moves the *bond parameters*, because `palw_v2_params_on_base` computes `bundle.bond = palw_v2_bond_outlasting_da_court(&bundle, base.palw_da_court)` (params.rs:8663-8664), raising `withdrawal_delay` past liability + the DA lattice. testnet-11 keeps 7,500. So the card and the RC do **not** ship "the same ruleset with the same fences armed" — the delay differs as a derived consequence of the fence, which the closure pass records nowhere. A rehearsal gap: whatever testnet-11 proves about court/retirement interleaving is proved at a different delay than a card runs. |

### Confirmed still open, exactly as described

O-3 (and **worse on a card**: squatting the global court-session cap now also blocks the k-ary court
whose silence convicts), O-5, O-6, O-9, O-10, O-11, O-13, O-15. No follow-through commit touches
`palw_attempt_v2.rs`, the `processor.rs` sortition, `palw_fp_devnet_v3.rs`,
`ghostdag/protocol.rs` or `dns_finality.rs` at the named sites.

### The shape this pass wants recorded

The fence doc at `params.rs:1023-1030` cites audit3 H4 — "the court's opening rung convicted
producers of classes for which no responder existed" — and the mercy arm at `:7203-7211` records the
same finding a second time ("two of this chain's three genesis classes … have no
`bisect_prefix_state` in this tree"). Both remediations were written for the k-ary ladder's **round
0**. The 2026-09-06 pass then armed a second court (DA) and a terminal move (fused) that reuse the
identical "silence convicts" arithmetic at points the round-0 remedies do not reach — and the round-0
mercy is in fact a no-op at **every** fused Terminal for **every** class, because a converged ladder
has `round() > 0` by construction. A responder-coverage predicate belongs in `validate_palw_v2`, as a
consensus-visible fact. Absent that, the next fence armed over a family gap will fail the same way
and no test in this tree will see it: the set-difference test compares card-vs-RC *arming sets* and
asks nothing about who can answer.

---

## Removed by the second lens

**Nothing was removed outright.** The second lens ran on ten CRITICAL and HIGH survivors and
returned five CONFIRMED, four SPLIT and one demotion. Recorded here as evidence about the method,
not hidden.

* **Demoted: freeprompt-claims-2 (M-3), HIGH → MEDIUM.** The mechanism survived a fresh derivation
  intact. The payoff did not: the immunity it buys is worth at most half a floor-level bond per
  claim over a 4,200-DAA window, on a court whose shipped seat never accuses automatically by
  design, and the finding's claim that it is the backstop for C-3 is wrong on the court's own
  economics — a successful disclosure *slashes the accuser*, so the DA court was never a discovery
  route for the un-opened rows. Two of the finding's three novelty points also failed:
  `trace_chunk_count = 1` is the honest value at shipped decode budgets, and the "complete immunity"
  framing over-states the magnitude. What survived is a free, permanent, unilateral hole in a
  consensus validity rule, and a *better* argument than the finder's: writing 0 is weakly dominant
  for an honest producer too, so the field will drift to 0 with no adversary at all.
* **Materially corrected, kept: money-mint-1 (H-2).** "The normal state of a carded mainnet, from
  block 1, ~100% increase in emission" is false — the excess equals trailing bonded-stake growth
  over an R-DAA window, is zero on a static set and zero at genesis, and is dead past R ≈ 601. The
  correction cut the claimed reach and duration substantially; the finding survived because the
  attacker-timed variant still pays ~200% on locked capital, and because the second lens found a
  *larger* consequence the finder listed without following (the missing count cap → permanent halt).
* **Materially corrected, kept: money-mint-2 (H-3).** The finder's own scenario threshold (157) was
  6× too generous to the code; the real threshold at the mandated 12 validators is 26. Also
  corrected downward: the reserve drip contributes zero until a first slashing.
* **Materially corrected, kept: court-adjudicability-2 (H-4).** "It destroys the escrow every time"
  is false — `void_claim` is a delay, never a confiscation. Compensated by a cost the finder missed
  (each of the 32 burned draws is a full job re-execution, run synchronously in the panel loop).
* **Materially corrected, kept: freeprompt-claims-1 (C-3).** The "reorg weight purchasable" aim was
  narrowed to public issuance and `safe_weight` capture; and a strictly worse variant was found in
  which the on-chain commitment is byte-indistinguishable from an honest widest job.

**Removed by a refuter (one finding).** `p2p-dos-ibd-2` — a 64 MiB `PruningPointPalwState` reply
whose full `into_state_v2` rebuild runs before the root comparison. The mechanism and the ordering
are real, and the finding still dies on payoff: the same actor at the same position has a strictly
cheaper path to the identical outcome. `ibd/flow.rs:2549-2551` turns a ~40-byte `found: false` into
`PruningSidecarUnavailable`, aborting the victim's whole IBD, and `:2548`'s 600-second
`dequeue_with_timeout!` means simply never answering holds the single-flight IBD for ten minutes at
zero cost. Uploading 64 MiB to buy a bounded one-shot decode is a negative-amplification trade.

---

## Unverifiable

A limit is not a verdict. These are carried as limits, with what would settle each. None is promoted
to a finding.

**Consensus-splitting questions.**

1. **Can a pruned and an archival node derive different beacon/anchor facts for the same block?** —
   the 09-05 pass's "first thing to test next", and it is now *partially* settled.
   `default_backward_chain_iterator` is `BackwardChainIterator::new(store, from, ORIGIN, false)`
   (reachability.rs:169-171), so on a pruned node it terminates at the pruning point and
   `processor.rs:7177-7180`'s "the iterator really does reach genesis" is false there. The
   **harmless half is settled**: truncation can only *lower* `prev_attempt_daa` /
   `predecessor_daa`, both consumers are `>= slot` refusals (palw_freeprompt_v3.rs:611,
   palw_panel_v2.rs:504), and an honest witness is already below the slot — so the derived fact
   differs in value and no verdict can differ. The **dangerous half is open**: a pruning point
   sitting *above* the claim's slot would yield a different `beacon_block` (a different receipt
   lottery) or `anchor_block` (a different panel). No reachable instance was constructed — the
   lattice windows (anchor_delay 20, window_bind 600, receipt_maturity 400, use_window 600) are
   three orders of magnitude shorter than any V2 pruning depth. *Settle it by:* (a) determining
   whether `PruningPointPalwState.classCarriages` can carry a still-`Provisional` claim across a
   pruning point, which would make it reachable on the anchor walk; (b) a two-node harness (one
   `--archival`, one pruned past the slot) comparing `PalwBeaconFactV3` / `PalwAnchorFactV2` field by
   field.
2. **Can two consecutive chain blocks share a DAA score?** *(MARKET)* — `write_model_fee` keys a
   payout row as `H(class_id, holder, daa_score, model_moves, leg)` and `write_payout` **replaces**
   on collision (palw_state_v2.rs:6232-6237, 6337-6344), silently destroying the earlier row;
   `model_moves` restarts at 0 in every block. Because `holder` is a free field on `ModelBuy`, an
   attacker could aim the collision at someone else's pending row. `internal_calc_daa_score` gives an
   increment of ≥1 unless the selected parent itself falls outside the DAA time window. *Settle it
   by:* a two-block fixture whose child's `mergeset_non_daa` contains its selected parent; if
   reachable, key the payout by block hash instead.
3. **Does a node that received the crashing block of C-4 re-validate it on restart?** The second
   lens argued yes from ordering (body validation passes with a small committed mass, so the block is
   stored and relayed before the UTXO stage panics), but the block-store commit ordering relative to
   `verify_expected_utxo_state` was not traced. *Settle it by:* reading the commit ordering in
   `body_processor` and `virtual_processor::process`, or running the two-block sequence and
   restarting.

**Cost and rate questions.**

4. The steady-state magnitude of R = daa_score − blue_score on a carded mainnet, which decides how
   long H-2's over-mint window stays open. Sign is certain; magnitude is not. *Settle it by:*
   replaying live testnet-11 headers and plotting R against height.
5. The sustained (as opposed to burst) heartbeat rate achievable under L-1's backdating band. *Settle
   it by:* simulating the 11-of-27 central average under an adversarial timestamp sequence.
6. The actual fabrication rate in C-1 (derived, not measured), and whether an attacker can sustain
   180 fabricated rows per chain block long enough to matter to the DAA-denominated deadlines, or
   only for the two or three blocks it takes `retarget_bits` to catch up.
   `retarget_bits_from_rows` (difficulty.rs:370-396) already exposes exactly this.
7. Throughput of the M-5 / H-1 amplification, and whether a padded 64 MiB carriage's
   `assert_internal_consistency_v2` walk is expensive enough to be meaningful griefing. Established
   as amplification *shape*, not measured throughput.
8. The exact sompi cost of one `DefaultAccused` transaction at the card's mass rules, and the
   `slash_bond` clamp when a reservation was already released by another path.
9. The storage-mass price of a model sink output, which bounds how cheap the M-13 UTXO-bloat channel
   really is; and whether any node-level supply or 10 B-cap assertion would fire on M-12 (a zero-grep
   is not proof). *Settle the latter by:* running the premine/cap tests against a fold that has
   executed one buy and one sell with the market armed — if none of them read the coinbase's extra
   outputs, the mint is invisible to the whole suite, which is itself the finding.

**Coverage questions the lanes could not close.**

10. **Which retention form a graph-v5 producer actually writes for a licensed *attempt* claim.**
    `refutation_with_prompt` decodes with `base0_material_decode_v1` (a bare `borsh::from_slice`,
    no magic check) while the fold form carries `PALW_BASE0_FP_MATERIAL_MAGIC_V2` and is read only by
    `base0_material_decode_any_v1`. If the retention is a fold, the responder cannot assemble a close
    at **any** index — which would make C-2 fire on in-range terminal leaves too and mean the court
    cannot convict a real fraud on that class.
11. **Whether a producer's on-disk retention survives to `claim.trace_retention_daa`.** The chain
    refuses an accusation whose disclose window runs past the claim's declared retention, but nothing
    in consensus can see the node's own pruning policy. If the panel's retention pruning is shorter
    than the chain's obligation, an accuser that waits convicts honest producers for free.
12. **Whether a carded mainnet's genesis class registrations are judged under ADR-0077 Phase B's
    ladder rules at all.** The card arms `palw_context_ladder` from genesis and
    `palw_class_admission_v2.rs:437` gates the Phase B rules on it, but `validate_palw_v2` states
    that genesis registrations are verified against the committed catalog rather than through
    `verify_class_admission_*` (params.rs:1673-1680) — which suggests the genesis rows may bypass
    rules every later entrant must satisfy.
13. **Whether arming `palw_panel_da` without `palw_da_court` is a reachable configuration with real
    harm.** No rule in `validate_palw_v2` relates them; a testnet-11 flag day could arm the first
    alone — prompts off chain with no court to prosecute withholding, and `palw_unavailable_abstains`
    making the seats' only other remedy slashing-free.
14. **The `at_two_minute_cadence` rescale audit nobody has done.** The function moves eleven cadence
    fields and its comment names exactly one deliberate omission
    (`reward_uniqueness_window_blocks`) — but it silently changes the meaning of every quantity
    coupled to `epoch_length_blocks` that it does not touch. M-1 is one case; the same question is
    open for `stake_score_window_blue_score` (30 after the rescale) against `required_stake_depth`
    (10 epochs), which `PRODUCTION_DNS_PARAMS`' own comment says the window must cover. Same failure
    shape at a security parameter rather than a money one.
15. **O-7's second harm** (free receipt blocks advance the DAA score, so DAA-denominated deadlines
    run ~180× fast). The fix addressed only the retarget-window half; the rate is bounded by the
    attacker's stock of *winning* quanta, and whether that stock is reachable at a scale that
    inflates the DAA meaningfully was not settled. *Settle it by:* measuring achievable receipt-block
    rate per epoch against `receipt_target` and `MAX_QUANTA_PER_RECEIPT` on the card's parameters.
16. **Whether a mergeset near 180 is offered in practice** on a carded mainnet, which is H-3's
    only soft input at low validator counts (at k ≥ 90 every block overflows regardless).
17. **Whether any shipped coinbase fan-out can actually emit a 1- or 2-sompi output**, the accidental
    (attacker-free) trigger for C-4.
18. **Whether an operator deployment of `kaspa-pq-signer` exists.** No client of the protocol is in
    this tree; zero grep hits is not proof of absence, and L-5's severity turns on it.
19. **Whether the same rollout-window split as M-9 exists at the wire level** for the thirteen new
    `PalwConsensusObjectV2` variants. Every object variant ever added to that enum has the property,
    so it may be a known and accepted consequence of the re-mint doctrine rather than new.
20. **Which processor.rs wiring a human picks** when resolving M-15's seven conflicts, and whether an
    operator resolving M-7's refusal moves `GENESIS.bits` (correct) or
    `MAINNET_PARAMS.max_difficulty_target` (silently reinstating the 256× wedge). Note the
    discriminator worth adding to the runbook: `consensus_params_id` hashes `max_difficulty_target`
    (params.rs:3258), so the wrong remedy at least moves the fingerprint the operator re-pins.

---

## Coverage and what this audit did not read

### What was read end to end

**money-mint** — `coinbase.rs` whole; `utxo_validation.rs:290-1440, 2770-2880`; `dns_finality.rs`'s
reward/mint region and its tests; `premine.rs` whole; the card assembly and `PRODUCTION_DNS_PARAMS`;
`tx_validation_in_isolation.rs`'s coinbase and DNS-tx rules; the `palw_state_v2` escrow/payout
mechanics; `processor.rs`'s escrow/entitlement wiring; `palw_fp_admission_v3.rs` whole.

**weight-and-time** — the card assembly and fence table; `difficulty.rs`, `window.rs`,
`past_median_time.rs`, the three header-validation stages, all whole; `ghostdag/protocol.rs`'s lane
blue work; `palw_chain_weight.rs`, `palw_fork_choice.rs`, `palw_fork_authority_v2.rs` whole;
`processor.rs`'s tip weights, sink search and parent selection; `pow_layer0.rs`,
`consensus/pow/src/lib.rs`, `palw_attempt_v2.rs`, `palw_pwu.rs`, `palw_heartbeat_v1.rs`,
`palw_class_daa.rs`, `palw_admission_v2.rs` items 1-9.

**court-adjudicability** — the prior audit whole; the card assembly and its fence tests;
`palw_bisect.rs`; `palw_court_v2.rs`'s open/close/adjudicate; the `palw_state_v2` court fold arms,
deadline machinery and sweep; `palw_step_leg.rs`'s walkers; `palw_step_refute.rs`'s opened-capped
entry and the DA disclosure forms; `palw_court_deadline.rs`; `palw_context_ladder.rs:130-540`;
`kaspad/src/palw_panel.rs`'s court arm, DA responder and seat verdict arm;
`misaka-palw-base0`'s backend/produce/legs/fp_capture seams; the processor's accepted-objects court
arms and conviction path.

**freeprompt-claims** — `palw_freeprompt_v3.rs`'s object/validation half whole;
`palw_fp_admission_v3.rs`, `palw_fp_execution_v3.rs`, `palw_fp_beacon_v3.rs`,
`palw_fp_interval_v1.rs` whole; the D8 pin table and DA fields of `palw_attempt_v2.rs`; the
`FreePromptCommitted`/`apply_receipt_spend`/`DefaultAccused` fold arms; `palw_panel_v2.rs`'s quorum;
`kaspad/src/palw_fp_seat.rs` whole and the panel's seat-duty loop; `misaka-palw-base0`'s
`fp_interval.rs` geometry and both verify paths, and all three backends at the same four seams; the
processor's beacon/anchor fact derivation and receipt-spend wiring; both v8 gossip flows whole.

**p2p-dos-ibd** — `palw_gossip.rs` whole; the five v8 flows; `ibd/flow.rs` and `negotiate.rs`;
the p2p router and connection handler; the pre-PoW gates and the PoW finalizer's handling of
peer-controlled fields; the wire→Header decode; the object-acceptance loop and the pruning-point
PALW state path; the RPC surface and the gateway's HTTP caps.

**carding-activation** — the prior audit whole; every card/RC assembly function and the whole
`consensus_params_id_tests` card block; `fork_id_v1.rs` whole and its handshake call site;
`premine.rs`, `genesis.rs`'s bits table; the court-ladder resolution on both trees; MARKET's three
new fence fields, their accessors, the eight new state tables, `state_root` and the hand-written
tagged-tail Borsh impls; and an **off-tree three-way merge** of both branches (34 files changed on
both sides; 13 conflict in 51 hunks; 21 merge silently).

**crypto-tx-validity** — `sighash.rs` whole (both transcripts); `mass/mod.rs` whole; `muhash.rs` and
the muhash crate; the txscript policy, script classes, standard scripts, runtime sig-op counter and
the CHECKSIG family; the addresses crate; all three transaction-validator modules; the mempool
standardness and admission ordering; `utxo_validation.rs`'s state calculation and flag choices; the
EVM bridge effects and withdraw invariants; the PQ signer and validator-core; the CLI key handling;
`host_security.rs`'s signing-secret reachability; the V2 signature contexts and every consumer.

**model-market-evm** *(MARKET tree)* — `palw_model_market_v1.rs` and `evm/model_market.rs` whole;
the market/registry half of `palw_state_v2.rs` including all thirteen object arms, the fee/position
writers, `evm_view_v1`, the payout drain and the in-file test modules; the processor's rehearsal
loop, market acceptance arms and EVM chain-context step; `processes/evm/mod.rs`;
`kaspa-evm/src/executor.rs`'s MarketSettle application and supply accumulator;
`kaspa-evm/src/model_market.rs`'s handlers and `write_frame`; the lifecycle may-ride table and
carrier binding; the three fences and their validation clauses; the handshake gate; the coinbase
assembly; ADR-0087 §Constraints/Decisions/Invariants and the 0088/0089/0090 implementation tables.

**prior-open-closure** — the prior audit whole; all 15 follow-through commit messages and two full
diffs; the card assembly, fence table and set-difference test; the DA accusation/disclosure/sweep
path and all four `PalwExecutionBackendV1` implementors; the k-ary terminal clock; the panel's
court-move arms and `palw_court_duties_v2`; the capability path; the O-1 cap threading and all
callers; the O-18 predicate and all five call sites; the retarget arithmetic.

### What this audit did not read

* **The EVM execution engine proper** — only the market seam was read on both trees.
* **The ADR-0088 proposal / evaluation / version-eviction fold arms and the usage counters** — the
  market finder read their guards and explicitly declined their arithmetic. The market refuter flags
  this as deserving a lane of its own before arming.
* **The read-precompile ABI encoders** (`kaspa-evm/src/model_market.rs` registry/amm/position/facade
  bodies). Declined as "a wrong answer to a caller, not a consensus fault" — which the refuter
  correctly says understates it once ADR-0089's facade exists, since `market_row` / `position`
  answers are what an on-chain contract prices against.
* **`mining/src/evm_mempool.rs`** — not in any lane's coverage statement; found only by the market
  refuter, and it is where the ordering, per-sender caps and replacement rule live. It falsified one
  finding's load-bearing claim (L-6).
* **Host security** — the 09-05 unjudged item at `palw_panel.rs:540` (the fee-funding recovery scan
  excluding one bond outpoint rather than the locked-collateral set) is untouched by this pass.
* **Executor determinism and the kernels** — `engine_a16.rs`, `kernels.rs`, `fp_recompute.rs`'s
  arithmetic. The 09-05 pass's item 5 lived here.
* **The pruning proof** beyond the IBD carriage path.
* **`palw_attn_court_v1.rs`'s dissection arithmetic** — read only at its entry from
  `adjudicate_court_close_capped_v2`, on the ground that no shipped binary can open a dissection at
  all (C-2).
* **The five other 09-05 unjudged items' sites** (`fp_interval.rs:1849`,
  `qwen25_a16_backend.rs:1483`, `palw_state_v2.rs:9395`, `palw_admission_v2.rs:357`,
  `palw_state_v2.rs:7698`, `dns_finality.rs:6886`, `palw_e2e_adjudicability.rs:847`,
  `palw_gossip.rs:716`) were each picked up by the lane that owns the dimension — four of them are
  now findings (M-2, M-3, M-4, H-2) and one was **refuted** (`palw_state_v2.rs:7698`: a gap epoch
  would need a chain step whose DAA advance exceeds `epoch_length`, and the advance is bounded by
  `mergeset_size_limit = 180` against `EPOCH_LENGTH = 1_000`, so `closed_epoch` moves by exactly one
  at every boundary; both `apply_class_share_growth` and `apply_class_reclamation` refuse a budget
  table stamped for another epoch).
* **Nothing was built or run.** Every figure is arithmetic over constants or a reading of a call
  chain. The 09-05 pass's ADR-0084 §7.2/§7.4 measurements are the only empirical inputs used.

**One process note for the next pass.** Both trees were left byte-identical to their pinned tips.
The 09-05 pass's first method fault — "the tree was edited while the audit read it", so a refuter
citing a guard was reporting that audit's own fix — did not recur, and it is worth keeping as a
standing rule: the audit reads, the repair happens after.
---

## Note added at filing, after the run (2026-09-06)

Filed by hand once the workflow returned. Three things a reader needs that the run itself could not
know, kept separate from the body so it is clear no agent wrote them.

1. **The pins, and a tree this audit did not read.** Every `file:line` above is against COURT
   `3c6d6747` and MARKET `8146e659`. *While this audit was running*, MARKET was merged into
   `palw-t11-da-court-market`, which is now at `dfdab5fb`. **The merged tree is not what was read.**
   This matters most for the `carding-activation` lane, whose cross-branch question ("which fence
   ends up unarmed when these branches merge, which test fails, which test silently passes") was
   answered by predicting a merge that has since happened: check its prediction against the merge
   commit itself before trusting it either way. Line numbers elsewhere will have moved; check out the
   two pins above to follow a citation.

2. **C-4 was re-verified by hand before filing.** `utxo_plurality` (`consensus/core/src/mass/mod.rs:64`)
   sums `UTXO_CONST_STORAGE` to 95 bytes against a `UTXO_UNIT_SIZE` of 100, so any SPK whose script
   exceeds five bytes has plurality 2 — which a PQ P2PKH script exceeds by orders of magnitude. The
   doc comment three lines above still asserts the upstream invariant this fork breaks ("The choice
   of 100 bytes per unit ensures that all standard SPKs have a plurality of 1"), while a comment
   inside the same function records why it cannot hold here ("kaspa-pq has no 33-byte 'standard'
   SPKs (PQ public keys are far larger)"). Two comments in one function, one of them false, is how
   the arithmetic path at `:406-409` came to be reachable at all. The stale comment is worth fixing
   in the same change as the division.

3. **The trees were not edited during the run.** That was the whole point of pinning them, and it is
   the difference between this report and the previous one, whose refuters cited its own in-flight
   fixes as acquittals. Both worktrees are still checked out at their pins for follow-up; remove them
   with `git worktree remove` when the findings have been worked.
