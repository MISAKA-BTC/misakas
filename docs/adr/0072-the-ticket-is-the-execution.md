# ADR-0072 — The ticket is the execution: both lotteries priced in inferences

Status: **IMPLEMENTED (2026-09-02); Decision 7's rollout DECIDED the same day — see §3.** Reviewed the same day
by a second session, whose finding became Decision 8 and the invariant in §4. Builds on ADR-0042 (Decision 3a: the algo-6 tag is an
expansion, never an inference), ADR-0044 (Decision 4: the receipt lane's quantum ticket is a
function of the certified execution), ADR-0045 (`DerivedV1`: pwu is derived from the class target),
ADR-0058 (merged work is counted) and ADR-0071 (Decision 1 withdrawn — `bits` stays the absolute
rate control; Decision 2 — the nonce bucket). Supersedes ADR-0071 Decision 2's pwu divisor; keeps
its bucket. Consistent with the standing doctrine that consensus changes ship by activation or
version, never by re-genesis: this is a version bump (`PALW_ATTEMPT_V2_VERSION` 5 → 6) and a
coordinated upgrade, not a re-mint.

> **Security amendment appended (2026-09-02)** — see the last section: the mainnet activation path §3 records must seed the new lane's targets (ADR-0076), ship the fork-id handshake before the fence, fence the version check, and prove cross-lane replay is refused by algo id.
> **Second pass (2026-09-03)** — the amendment was written before the code and the code was then reviewed adversarially. SA-1's window half is narrower than stated (nothing in this tree lane-filters a window), SA-3 binds on every peer-facing caller and not only the shape gate, SA-4 is a predicate that four sites had to ask, and SA-2's gate must refuse on the DISPUTED fence rather than on any fence. See the last section, including what still argues against arming.

## 1. The finding

An algo-6 header enters two lotteries: the class ticket (admission item 6b, against the class's
retargeted `initial_target`) and the Layer-0 digest against `bits`. Until this ADR both were drawn
from `commitment_root_v2`, the attempt's identity root — and the identity covers the `challenge`,
which is `H(network ‖ pre_pow ‖ timestamp ‖ nonce ‖ class ‖ bond)`. So every nonce was a fresh
draw in both lotteries against one inference. ADR-0071 Decision 2 bounded the sweep to a bucket
of `2^22` nonces so that a verifier could name which execution a nonce was paid for by, but inside
the bucket the sweep stayed free: the public producer ran exactly `4_000_000` nonces per template
per inference, and the audit that produced ADR-0071 measured it.

Two things followed from drawing the lottery off the wrong bytes:

* the class retarget priced a sweep, not an inference — a class's difficulty said how many
  hashes its blocks cost, and the LLM work behind them was a constant one;
* pwu had to be patched to divide the expected tries by `2^22` (ADR-0071 Decision 2's formula),
  an accounting correction for a lottery that should never have counted tries.

The receipt lane never had this problem. ADR-0044 Decision 4 drew its ticket from the certified
quantum and the chain's beacon, so the only way to another draw was another execution, and its
nonce was a uniqueness field from the day it shipped. This ADR gives the attempt lane the same
discipline.

## 2. Decisions

**Decision 1 — The priced bytes are the execution.**
`execution_commitment_v3(attempt, anchor) = H(domain ‖ anchor ‖ borsh(attempt with challenge := 0))`.
The attempt struct is not changed and no second projection is written: the field that changes per
nonce is blanked and everything else is priced, so a field added to the attempt tomorrow is priced
the moment it exists (the P0-1 discipline `commitment_root_v2` already keeps). `commitment_root_v2`
and `attempt_id_v2` are unchanged and remain the block's IDENTITY: two nonces are two blocks.

**Decision 2 — The anchor is derived, never carried.**
`execution_anchor_v3(network_domain, pre_pow_hash, class_id, bond, nonce)` is
`palw_job_anchor_v1(…, palw_nonce_bucket_v1(nonce))` — the job anchor the panel and the court
already derive from a header. Every verifier computes it from the header it holds: the finalizer
arm (on every path that computes PoW, the pruning-proof path included) and the composed admission
entry point. It is not a field of the envelope, so the accused does not get to set the question,
and the `challenge` equation is still checked at both places so nonce and timestamp remain bound to
the position while buying nothing.

**Decision 3 — Both lotteries draw from it.**
The class ticket is `class_ticket_v3(attempt, anchor)`, the low 128 bits of
`H(domain ‖ execution_commitment_v3)`. The Layer-0 digest for algo-6 is
`finalizer(network, algo, pre_pow, timestamp := 0, bits, nonce := 0, Expand(execution_commitment_v3))`
— nonce-free and timestamp-free — and it is still compared to `bits`. ADR-0071's withdrawn
Decision 1 is why: `bits` is the only absolute control on block interval (the class retarget is
relative), and the measured floor of 50 blocks/min is what removing it did. It is kept, and it is
priced in inferences now, which is what it was always meant to price.

**Decision 4 — The class lottery moves beside the position.**
Item 6b leaves the envelope-only stateful list (`check_palw_attempt_admission_v2`, which the state
machine re-runs as a transition guard with no header in hand) and becomes
`check_palw_class_lottery_v3(state, attempt, anchor)`, called from the composed entry point after
the stateless list has agreed that the carried domain and challenge are this network's and this
position's. Every attempt passes the composed entry point exactly once — the chain block's own at
the virtual processor, each merged block's at ADR-0058's pre-check — so nothing is lost; what
changes is that the lottery is treated exactly as the network lottery always was: a header-level
fact, checked where the header is and never re-derived in the transition.

**Decision 5 — One draw is one execution.**
`palw_pwu_v1(target, per_inference) = max(1, expected_draws(target)) × per_inference`. ADR-0071
Decision 2's `>> PALW_TICKET_NONCE_BUCKET_LOG2` is withdrawn (its text stays in that ADR as the
record); the bucket constant itself stands, because it is what names which execution a nonce was
paid for by. The `DerivedV1` equality (ADR-0045) is untouched — a producer still claims exactly the
derived value.

**Decision 6 — The producer: one template, one inference, one draw.**
The nonce search is deleted. `produce_one` derives the bucket from a cursor, runs the inference
for that bucket's job, builds the attempt with the challenge for `nonce = bucket << 22`, and
decides both lotteries with two hashes. The engine is deterministic, so `(template, bucket) → job
→ execution → ticket` is a function: a bucket a template has lost stays lost, and the cursor is
what keeps a producer from re-running it — a template with the same pre-PoW hash resumes at the
next bucket (the timestamp is outside `pre_pow_hash_64`, so a refreshed template with the same
parents continues rather than replaying), any other template starts at zero.

**Decision 7 — Version, and how this goes live.**
`PALW_ATTEMPT_V2_VERSION` 5 → 6. A node on the old rule draws a ticket per nonce and admits blocks
this rule refuses at the class lottery, so the two cannot share a chain; the version check keeps
them from trying. The shipped fingerprints move on the two presets that carry a V2 bundle
(testnet-11, devnet), because the attempt version is inside the bundle's ruleset id.

The first draft of this decision said "testnet-11 upgrades together; no re-genesis" and left the
rest to the DAA. The review showed that is not a rollout, for two reasons that are stated here
rather than fixed here, because the fix was the operator's choice (§3, now decided):

* **The version check is not fence-gated.** A node on this build refuses every version-5 envelope,
  which is every attempt block the chain already holds — a fresh node cannot validate the history
  it is asked to sync. Every earlier attempt-format change shipped with a re-genesis for exactly
  this reason.
* **`bits` cannot relax.** The swap divides the network's draw rate by ~2^22 while `bits` is
  inherited from the old rate. The difficulty window has no lane filter, so it keeps moving only
  through heartbeat blocks, and each heartbeat adds at most (heartbeat interval / target interval)
  ≈ 3,000 of span per block of work; the relaxation is therefore bounded at ~3,000× however long
  one waits, against the ~10^6× the swap needs. The attempt lane would be dead, not slow — the
  algo-6 dead time after the swap is unbounded.

**Decision 8 — Every field inside the priced bytes is pinned, or it is the challenge.**
"Priced" is not "pinned." A field the producer chooses freely and no rule pins is a nonce by
another name, and the review reproduced it: sweeping `trace_retention_daa` over ONE execution gave
4,096 distinct tickets and 4,096 distinct Layer-0 tags and admitted nine of them at a 2^-9
target, with honest roots, so the panel had nothing to convict — 2^64 free draws on both
lotteries, and with Decision 5 in force each ground block also claimed the full
`expected_draws × per_inference` as pwu. Nothing read `trace_manifest_root`; nothing pinned
`trace_chunk_count` beyond `!= 0`. So the three DA fields are pinned at the composed entry point
(`check_palw_attempt_da_pins_v1`): `trace_chunk_count == 1` (the one shape every shipped family
serves), `trace_manifest_root == attempt_trace_manifest_root_v1(trace_root, count)` (one
derivation in consensus, which every family now uses — theirs hashed a job context under a family
domain that no verifier ever read), and `trace_retention_daa == the block's own DAA score +
palw_min_trace_retention_daa_v1` (the obligation the chain defines, derived not chosen; a merged
block's own score, not the accepting block's). The invariant is stated as a test that classifies
every field of the struct exhaustively — chain equality, execution replay, derived, or the one
position field — so a field added tomorrow does not compile until it is placed.

## 3. What this costs, stated before it is measured — and the rollout choice

**Rollout (Decision 7, decided 2026-09-02): this rule goes live inside Relaunch 5's re-genesis,
with no fence.** The operator's decision, taken with both paths below on the table: Relaunch 5
was already pending for the A16 re-pin and ADR-0069's certified set, ADR-0073 ③ (the attempt lane
drawn by the chain's own beacon) lands in the same genesis, and a scheduled activation would have
carried dual finalizer arms, dual admission paths and a lane-filtered difficulty window into a
network that is re-minting anyway. So: genesis `bits` resets the network lottery; there is no
version-5 history to validate; the deployment is a rolling swap in the shape of the 08-29
re-mint — the fingerprint moves, so an un-wiped old peer is refused at the handshake and cannot
re-feed the old chain. The activation path stays recorded as what mainnet will have to do, because
mainnet cannot re-mint (2026-08-27 doctrine); the first draft of it (algo id 9 behind a DAA fence,
the algo-6 arm and its envelope-only lottery kept byte for byte, `pwu` re-derived per lane, the
difficulty window restarted on the new lane) was built far enough to know its shape and then
withdrawn from this branch unmerged.

Two ways this rule could go live, and only two:

* **(a) Relaunch 6.** A re-genesis resets `bits` to the genesis value (~20 draws) and leaves no
  version-5 history to validate. Zero code. It is the re-genesis the standing doctrine
  (2026-08-27: consensus changes ship by activation, never by re-genesis; mainnet cannot re-mint)
  says to stop doing.
* **(b) Activation.** A new algo id for the execution-priced lane, gated by a DAA fence, with the
  algo-6 arm and its envelope-only lottery kept byte-identical for history; the difficulty window
  filtered to the new lane, so it restarts from genesis `bits` at the fence the way the chain
  restarts at genesis (fewer than the minimum window of new-lane blocks → genesis `bits`). The
  doctrine's answer and ADR-0066's shape, at the cost of dual arms and dual admission paths, and
  the fingerprint still moves at the swap (the fork-id handshake that would let it not move is a
  later ADR). Roughly the size of this ADR again.

The rule is the same under both; what differs is whether the chain's history survives the swap.
On testnet-11 it does not, by decision; on mainnet it must, by doctrine.

Expected inferences per block per producer = `1 / (P_class × P_bits)`. At the RC genesis the class
target is `2^-1` and the genesis `bits` is about `1/20`, so a producer expects ~40 inferences per
block: at 9 s per QWEN36 inference that is ~6 minutes per block per producer, and the BASE-0 floor
at milliseconds per inference is bounded by its epoch budget rather than by the lottery. The
network's draw rate fell by roughly `2^22`, so the DAA will relax `bits` toward its trivial cap;
at the cap every draw passes the network lottery and the chain's cadence is bounded by the class
lotteries, the share table and the inference supply. That is the intended state: cadence is a
function of how much LLM work the network can do, and nothing else.

What this does NOT do: it does not make real-demand (free-prompt) work the primary lane — the
attempt lane is still the canonical-job lane — and it does not change the receipt lane at all.
Those are the next question, and this ADR is its foundation, because a lottery priced in
inferences is the only kind that can be handed to real work.

## 4. Invariants the tests hold

* `every_priced_field_moves_the_pow_tag`: every field but `challenge` moves the PoW tag;
  `challenge` moves the block identity and leaves the tag exactly where it was.
* `the_anchor_binds_the_execution_to_its_position_and_nothing_inside_a_bucket_moves_it`: every
  anchor input (network, template, class, bond, bucket) moves tag and ticket; three nonces of one
  bucket move neither.
* `palw_v2_commitment_mutation_invalidates_pow`: nonce + 1 and timestamp + 1, honestly re-derived,
  give the same Layer-0 digest; the next bucket gives a different one; a challenge that does not
  match the header is refused on every PoW path.
* `the_class_target_is_what_admits_a_block_of_that_class`: two nonces, one ticket; two executions,
  two tickets; the ticket is not the tag under another name.
* `pwu_separates_cost_from_difficulty`: an 8× tighter target is 8× the pwu at every scale — the
  bucket no longer flattens the curve — and the easiest target is still one execution.
* The integration tests that produce blocks from real executions draw per bucket: a lost draw is a
  second real inference, never a nonce.
* `every_priced_field_is_pinned_or_is_the_challenge`: every field of the attempt is classified —
  chain equality, execution replay, derived, or the one position field — and the derived ones
  derive.
* `a_free_field_inside_the_priced_bytes_is_a_nonce_by_another_name`: sweeping each DA field over
  one execution reaches the lottery for exactly the one derived value; every other value is
  refused at the pin, by name.

## 5. Supersession

| Decision | Status after this ADR |
|---|---|
| ADR-0042 Decision 3a — tag is `Expand(commitment_root_v2)` | tag is `Expand(execution_commitment_v3)`; 3a's guarantee ("a solved header attests exactly one attempt at exactly this position") is kept by the challenge equation plus the derived anchor |
| ADR-0071 Decision 2 — `executions = tries >> k` | withdrawn: `executions = tries`; the bucket `k = 22` stands as the anchor's position field |
| Admission item 6b in the envelope-only list | moved to `check_palw_class_lottery_v3`, called from the composed entry point |
| The three DA fields, producer-chosen | pinned by equality at the composed entry point (`check_palw_attempt_da_pins_v1`); every family derives the manifest the consensus way |

## Security amendment (2026-09-02) — the mainnet activation path, before it is built

**SA-1 — The new lane is seeded, not restarted.** At the fence, every class's new-lane target is
`attempt_target_seed_v1(share, pwu_per_inference)` from the fold at the fence (ADR-0076
Decision 1), and the lane-filtered difficulty window starts from the pre-fence window's last `bits`,
not from genesis bits. A window restarted from genesis is the empty-chain start burst — an issuance
and reorg hazard on a network that cannot re-mint.

**SA-2 — The fork-id handshake precedes the fence.** A node advertises `(genesis, fired fences, next
fence)` and, past the fence height, refuses peers whose fired set differs. Without it the un-upgraded
minority extends the old arm, refuses the majority's blocks, and neither side sees a partition —
the silent-fork lesson this repository has already paid for. It is the precondition of any mainnet
fence, not a follow-up ADR.

**SA-3 — The version check is fenced.** §3's Decision 7 analysis found the attempt version check is
not fence-gated. A mainnet fence requires pre-fence history validated by the old arm and post-fence
blocks by the new, in one binary, pinned by a test that syncs a mixed history from genesis.

**SA-4 — Cross-lane replay is refused by construction**: the new lane's `algo_id` enters the
Layer-0 digest, so an attempt built for the old lottery is not a candidate in the new; a test
asserts a valid version-5 attempt is refused after the fence by algo id, not merely by version.

## Security amendment, second pass (2026-09-03) — what the implementation measured

The first pass wrote SA-1..SA-4 before the code existed. The code was then written, reviewed
adversarially, and the review found the amendment wrong in one place and the implementation wrong in
six. This section is the correction, so that the divergence lives in the ADR rather than only in Rust
comments.

**SA-1 is one rule, and its second half is a constraint on a filter nobody has written.** The seed
half stands verbatim: at the fence a class's new-lane target is
`attempt_target_seed_v1(share, pwu_per_inference)` from the fold at the fence — the *same function*
a post-genesis admission calls, not a second rule that agrees. The implementation briefly carried a
wrapper (`attempt_lane_target_seed_at_fence_v1`) and an identity function
(`attempt_lane_window_seed_bits_v1(a, b) = a`), each with a test asserting what an alias and an
identity cannot fail; both are deleted, because a ceiling nobody reaches is not a rule.

The window half is narrower than this ADR stated. **No difficulty window in this tree is
lane-filtered at all** — nothing in `consensus/src/processes/window.rs` or `difficulty.rs` reads
`pow_algo_id` — and `SampledDifficultyManager::calculate_difficulty_bits`'s short-window fallback
already returns the *selected parent's own* `bits` unless that parent IS genesis. At a fence on a
live chain the selected parent is never genesis, so the pre-fence `bits` is what the fallback yields
for free. The hazard therefore bites only if someone later writes a lane filter that RESETS the
window rather than emptying it. **The rule, for whoever writes it: a lane-filtered attempt window
starts from the pre-fence window's last `bits`, never from genesis bits.**

**SA-3 binds on every peer-facing caller, not just the shape gate.** Fencing
`check_palw_commitment_shape_at` was not enough: `check_palw_carriage_stateless` called the
un-parameterised `validate_stateless_v2` forty-five lines later, so on an armed network below its
fence the fenced gate admitted a legacy envelope and the un-fenced check refused it — Decision 7's
defect reproduced by its own fix. The relay path now takes `PalwAttemptLaneV1::attempt_version()`
(`validate_stateless_v2_at_version`). **Still compiled in at the current version:** the stateful
admission (`check_palw_attempt_admission_full_v2*`). See "What still argues against arming" below.

**SA-4 is a predicate, not a constant, and four sites had to ask it.** "Which id carries the attempt
lane" is `pow_layer0::is_palw_attempt_algo_id`. Four rules were spelled as equality against the old
id and each broke differently at the fence:

| Site | What the stale spelling did past the fence |
| --- | --- |
| `pre_ghostdag_validation::check_palw_carriage_stateless` | fell to `_ => Ok(())`: no envelope binding and **no ML-DSA signature check**, so one solved PoW minted unbounded distinct blocks (launch blockers §5, on the new lane) |
| `kaspad::palw_producer::produce_one` | hard-errored on every template, so **every producer on every node stopped at the same DAA score**, unrecoverably (`PalwRulesetV2::validate` pins the bundle's `algorithm_id` at 6, so no configuration reaches 9) |
| `ghostdag::protocol` blue work | attempt blocks stopped earning ADR-0068 F2's fixed ε and started earning `calc_work(bits)` — fork-choice weight re-priced at one height as a side effect of an id change |
| `virtual_processor::palw_beacon_fact_of_candidate` | the ADR-0044 beacon walk saw the attempt chain stop at the fence; `prev_attempt_daa` froze permanently |

The beacon walk is the one consumer that must accept **both** ids at once, because it descends
through the fence; the other three resolve the lane at a single height. Sites that legitimately keep
naming algo-6 are the ones that mean *this specific algorithm*: the V2 bundle's own `algorithm_id`
(`PalwRulesetV2::validate`, the genesis bundles), and test fixtures that build a V2 network.

**SA-2's gate refuses on the DISPUTED fence, not on any fence.** As first implemented,
`evaluate_fork_id_v1` classified on "has this node crossed anything", and three shipped presets
already schedule crescendo (mainnet 110,165,000; testnet-10/-11 2,125,000). Arming ADR-0072's fence
at a future height therefore refused every un-armed peer from the moment the build started — in both
directions, for the whole rollout — which is exactly the M1-6 deploy-day partition SA-2 exists to
prevent. `fork_id_gate_fences_v1` now names the fences a refusal may rest on (crescendo is not among
them), and a `NextFenceDiffers` mismatch is tested against the height of the fence actually in
dispute.

### What still argues against arming this fence

* **At genesis (`ForkActivation::always()`) the fence is safe.** `LegacyArm` is unreachable, so none
  of the residuals below can occur, and `consensus_identity_id` already separates armed from
  un-armed builds (a genesis-active fence is a rule about block 1, not a schedule) — which is why
  `fork_id_gate_fences_v1` deliberately excludes height 0.
* **At a future height on a chain with real pre-ADR-0072 history it is not.** Two things behind the
  fenced gates still speak only the current rule: the stateful admission's `validate_stateless_v2`,
  and the pre-ADR-0072 lottery arithmetic itself, which was deleted at Relaunch 5's re-genesis. A
  legacy-arm block cannot pass PoW under this binary whatever version its envelope declares, so §3
  option (b) needs that arithmetic restored byte for byte before the fence can be scheduled ahead of
  a live chain. The shipped producer says the same thing from the other side: it stamps the current
  envelope version on both ids it can build for, and cannot build a `LegacyArm` block at all.

## Security amendment, third pass (2026-09-03) — the gate the sweep could not see

The second pass swept every comparison against `POW_ALGO_ID_PALW_COMMITTED_V2` and fixed four. A
fifth gate had the same defect and no such comparison, so nothing in that sweep could find it:
`consensus/pow/src/palw_admission.rs` called the shape rule through its **un-parameterised** entry
point, which supplies `PalwAttemptLaneV1::Unfenced` — that is, "algo-6 is the attempt lane at every
height" — as an omitted argument. Its live caller is `virtual_processor::utxo_validation`, which
runs for every chain candidate, and an `Err` there becomes `StatusDisqualifiedFromChain`.

Reproduced before it was fixed, at that exact call:

```text
UTXO ADMISSION(algo 6) = Ok(NotBound)
UTXO ADMISSION(algo 9) = Err(Commitment(PalwAttemptLaneClosed { algo_id: 9, open: 6 }))
```

On a network armed at genesis every attempt block declares algo-9, so **every chain candidate was
disqualified and the virtual chain never left genesis** — while the blocks still entered the DAG and
relayed normally, so the only symptom was `disqualified from virtual chain`, a line that never names
the fence. No operator configuration could escape it. That falsified the second pass's own
"conditional YES at genesis".

Three things changed, and the third is the one that closes the class rather than the instance:

1. `check_palw_block_admission_v1` takes an `attempt_lane` and uses `check_palw_commitment_shape_at`;
   the UTXO validator resolves it from `palw_attempt_activation` at the header's own DAA score, the
   way the relay gate and the pruning proof already do.
2. The four test-harness sites that stamped a carriage only for algo-6 were widened to the shared
   predicate — including `TestConsensus::build_header_with_parents`, which declares the id from the
   mode cascade, so an armed harness no longer builds algo-6 headers while the template builder
   builds algo-9 ones.
3. **The un-parameterised entry point is now `#[cfg(test)]`.** After (1) it had no non-test caller in
   the tree — the dead-code lint said so immediately — and a consensus gate that wants the shape rule
   must now call `check_palw_commitment_shape_at` and name the lane it means. That is enforced by the
   compiler rather than by the next sweep.

The evidence base is no longer only unit-level: `palw_v2_a_genesis_armed_attempt_lane_builds_a_chain`
boots a `ConsensusV2` network with `palw_attempt_activation = Some(ForkActivation::always())`, builds
blocks through the real pipeline, and asserts the sink leaves genesis on an algo-9 header. Without
the lane threaded into admission it fails with no body tip ever reaching `StatusUTXOValid`.

**Still not proven, and still fatal above genesis.** The harness runs under `skip_proof_of_work`, so
the algo-9 finalizer arm is not priced end to end here, and no *deployed* devnet has yet booted armed
at genesis. The `LegacyArm` residuals of the second pass are unchanged: the stateful admission
(`palw_admission_v2`) still validates at the compiled-in envelope version — correct for `Unfenced`
and `ExecutionArm`, wrong for `LegacyArm` — and the pre-ADR-0072 lottery arithmetic those blocks were
mined under is still deleted.
