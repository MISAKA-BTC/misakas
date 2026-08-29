# PALW — third-pass audit of the mainnet remediation, and the testnet-11 re-mint decision (2026-08-29)

**What this is.** A third read of the same changeset, run after
`docs/palw-mainnet-reaudit-2026-08-29.md` and against the tree that document's own repairs produced:
branch `claude/mainnet-audit-fixes-9g6oh9`, HEAD `65674f89`, 29 files, ~4,250 insertions over `main`
(`8e982b7`).

**Why there is a third pass.** The second one says, in its own method note, that it was
*"single-threaded and a sampling method, not a sweep"*, because its subagents could not call tools;
and that *"the M2 adjudication and transport surfaces were read at the two points the original audit
called deciding, and not exhaustively."* It also found its own deciding item — R-8, a chain-halting
critical — not by reading but by running a suite neither earlier pass had run. A pass that says that
about itself has named the thing to do next.

**Method, so the result can be weighed.** Ten independent lanes over the changeset, each told that
refutation is the default and that every "fixed" in both prior documents is an unverified claim.
Every finding then went to two adversarial verifiers with different lenses — *does the code do what
the finding says* and *is the consequence real at the severity claimed* — each instructed to answer
REFUTED unless it could not break the claim. A coverage critic then judged what the ten lanes missed.
85 agents, 2,675 tool calls. Of 37 raw findings, **11 were fully refuted, 6 survived one lens, and
20 survived both**. The refuted ones are not listed; the partially-refuted ones are, with both sides.

**Tree state as audited** (measured here, not carried over):

| | |
|---|---|
| `cargo build` (default members) | clean |
| `cargo build --workspace` | **fails** — `misaka-palw-worker` needs an external llama.cpp checkout; `--workspace` overrides `default-members`, which excludes it. Not a regression, but it is why "build clean" needs to say which invocation |
| `kaspa-consensus-core --lib` | 1347 passed / 0 failed |
| `kaspa-consensus --lib` | 251 passed / 0 failed |
| `misaka-palw-base0 --lib` | **162** passed / 0 failed (the re-audit records 163; 162 is what this tree produces, with and without `MISAKA_PALW_POW_FIXTURE=1`) |
| `kaspa-pq-validator-core --lib` | 36 passed / 0 failed |
| `misaka-cli` | 35 passed / 0 failed |
| both pinned preset fingerprints | match the code |

---

## Verdict

**Mainnet: NO-GO.** Four criticals and eleven highs survive adversarial verification, and all four
criticals sit on items recorded `fixed` in one of the two prior documents. The pattern is consistent
enough to be the finding: **the remediation closed the mechanism each audit named and left the
consequence open.** M2-3's pwu leg was closed and its anti-replay leg was not; M2-2's lock-across-IO
was closed and its unbounded-count was not; M2-5's conviction-on-silence was closed and its
no-responder half was not; M2-8's activation window was closed and the accumulation that replaced it
is worse than what it replaced.

**testnet-11 re-mint: NO-GO *for this build*, and the reason is arithmetic rather than caution.**
Several of the surviving findings are consensus-affecting — S-03's share table and S-04's coinbase
change which blocks are valid, and closing them moves `palw_ruleset_id_v2` and therefore the
fingerprint again. Re-minting testnet-11 is a destructive flag day: it discards the chain, and every
participant must wipe and rejoin. Spending one now buys a network that needs a second one as soon as
these are fixed. The genesis constants themselves are untouched by this changeset
(`git diff 8e982b7e 65674f89 -- consensus/core/src/config/genesis.rs consensus/core/src/config/premine.rs`
is empty), so the 347M community allocation is not at risk either way — but the chain since block 0
is, and it is not worth spending twice.

---

## What this pass fixed while auditing

**The R-3 acceptance test did not bite.** The re-audit's headline claim for it —
*"Verified to bite. Reverting `pruning_point_witness_child` to M1-2's heaviest-child rule and
re-running: the test fails"* — was established by reverting the rule and running the test **once**.
Run it ten times and it fails three to seven of them. All four children of the pruning point carry
identical blue work, so the discarded rule turns entirely on its hash tiebreak, and the block hashes
in this harness are not stable between runs: with three siblings against one honest child the honest
child holds the maximum roughly one run in four. The test was a coin flip, not an acceptance test,
and a regression it catches 75% of the time is one that gets merged. (The test's own doc comment said
so — *"the old rule makes this test flaky rather than red"* — while the commit message and the audit
report both claimed it bites. The code was honest and the report was not.)

Fixed by pinning the discrimination rather than sampling it: each sibling is now ground until it
out-ranks the chain child under exactly the comparison the discarded rule used, and the test asserts
that precondition before it asserts anything else. Measured: **10/10 red against the heaviest-child
rule, 10/10 green against the selected-chain rule**, where before it was 3/5 and 10/10.

**Two prior claims re-verified by execution rather than reading**, since that is what this pass is
for:

- R-1 bites. Reintroducing the single-sentinel normalisation turns
  `a_genesis_active_fence_is_not_normalised_away` and
  `turning_off_any_genesis_active_fence_moves_the_identity` red. Real guards.
- R-2's compile gate is real. Adding a `probe_new_fence_daa_score: u64` to `VltParams` produces
  `error[E0027]: pattern does not mention field` at `params.rs:1006` — the destructure is exhaustive,
  and the only `..` anywhere in `for_each_fence` is inside a comment.

---

## The findings

Numbered S-01…S-20 in severity order. Each survived two adversarial verifiers.

### S-01 `[critical]` M2-8's pending-share accumulation writes a not-yet-activated class's permille into the LIVE share table — the 1000‰ invariant breaks permanently and every pruning-point IBD is refused forever
`consensus/core/src/palw_state_v2.rs:4620`  ·  verifier lenses: critical, critical

**Recorded as fixed.** M2-8, recorded in docs/palw-mainnet-audit-2026-08-28.md remediation table as "fixed — activation
bounded to a window; pending grants count against the share table". The window half is real; the
accumulation half is implemented by merging pending shares into the table the transition then
WRITES, which introduces a worse defect than the one it closes.

**Mechanism.** The M2-8 remediation builds a merged view of live + pending shares and then hands the WHOLE result
to the live writer:

```rust
let mut share_view = builder.state.class_shares.clone();
for (id, record) in builder.state.classes.iter() {
    if let PalwClassStatusV2::Registered { pending_share_permille, .. } = record.status
        && !share_view.contains_key(id)
    { share_view.insert(*id, pending_share_permille); }
}
let table = granted_share_table_v2(builder.params, &share_view, *class_id, *share_permille)?;
let weightless = *activation_daa > ctx.daa_score;
if !weightless {
    for (id, share) in table {
        if builder.state.class_shares.get(&id).copied() != Some(share) {
            builder.write_share(id, Some(share));
        }
    }
}
```
(:4605-4625). `granted_share_table_v2` returns an entry for EVERY key of `current` (:3712-3730),
so `table` contains the pending classes that were merged in. `class_shares.get(&id)` is `None` for
them, `None != Some(share)`, and `write_share` inserts straight into `builder.state.class_shares`
(:2784-2791). A class whose status is still `Registered { .. }` therefore holds a live cadence
permille.

That is exactly what the state's own invariant forbids: `share_bearing` is built as `classes`
filtered to `!matches!(status, Registered { .. } | Dormant { .. })` (:2303-2309) and
`assert_internal_consistency` fails with `"the class set and the share table disagree — every
share names a class, and every class past its activation edge holds a share"` (:2310-2312) the
moment the sets differ.

The damage then becomes permanent at the activation edge. `activate_due_classes` grants against
the LIVE table — `granted_share_table_v2(builder.params, &builder.state.class_shares, class_id,
share)` (:3887) — and that table now already CONTAINS the entrant. The final
`table.insert(entrant, share)` (:3729) overwrites the entrant's scaled value `g`, so the sum is
`keep - g + share = 1000 - g` instead of 1000. Because a 1‰ holder's remainder `(1·keep) mod 1000
= 999` is the maximum possible, the entrant sorts first in the largest-remainder pass (:3719) and
takes a residue permille, so `g = 1` and the table sums to 999. The function's own
`debug_assert_eq!(table.values()… , 1000, "the donation arithmetic conserves the denominator")`
(:3745) is the code asserting the property this path breaks. `assert_internal_consistency` then
fails forever on `"the share table sums to {sum}‰ — the donation arithmetic conserves exactly
1000‰"` (:2313-2321); nothing anywhere restores a lost permille.

Nothing on the block path runs `assert_internal_consistency` — its only production caller is
`PalwStateCarriageV2::into_state` (:5461), reached from
`VirtualStateProcessor::import_pruning_point_palw_state`
(consensus/src/pipeline/virtual_processor/processor.rs:10629-10631) and from nowhere else. Live
nodes therefore accept and commit the corrupt state (deterministically — no fork), and the failure
surfaces only on the IBD path, where `protocol/flows/src/ibd/flow.rs:1838` propagates it with `?`
and aborts `IBD with headers proof`.

Acceptance does not stop it: processor.rs:4495-4625 checks only the target, the floor share, an
Active registrant bond, the signature and `verify_class_admission_v2` — `activation_daa` is read
but never constrained to 0, and the transition's only bound on it is the 4,000-DAA lookahead
(:4568-4575), which is what makes a weightless registration legal by design (ADR-0049 conditions
12/13). The sequential filter at processor.rs:4290-4310 rehearses with `apply_palw_transition_v2`,
which runs this same code and succeeds, so the object is accepted rather than dropped.

**Failure scenario.** Chain state: testnet-11/RC as `palw_rc_params_with_classes` mints it — class_shares = {BASE-0:
600, qwen36: 200, qwen25: 200}, min_grantable_share_permille = ceil(1e6/(1000·1000)) = 1‰,
registration_exposure = 40,000 sompi, min collateral 400,000. Attacker holds one Active bond and
two admissible profiles (e.g. the shipped BASE-0 geometry at two different `n_threads`, which
`validate_shape` and the coverage gate both accept and which yield two distinct
`shape_profile_id`s).

Inputs: one 0x4b lifecycle transaction carrying `ClassRegistered{ class_id: P1, share_permille: 1,
activation_daa: <block daa>+10, admission: Some(..) }`, then a second carrying `ClassRegistered{
class_id: P2, share_permille: 1, activation_daa: 0, admission: Some(..) }`. Both pass acceptance
(correct target, floor share, Active bond, valid signature, admissible graph, 80,000 ≤ 400,000
exposure).

Outcome, computed by executing the ported arithmetic:
1. P1 registers weightless — nothing written, `classes[P1].status = Registered{ activation_daa,
pending 1 }`.
2. P2 registers non-weightless. `share_view = {base:600, q36:200, q25:200, P1:1}`; the returned
table is `{P1:1, q25:200, q36:199, base:599, P2:1}` and ALL of it is written. `class_shares` now
contains P1 — a class whose status is `Registered`. From this block on,
`assert_internal_consistency` returns `CarriageInconsistent("the class set and the share table
disagree …")`, so `import_pruning_point_palw_state` refuses the pruning-point carriage and
`sync_and_validate_pruning_proof`'s IBD aborts against every peer (every peer serves the same
chain-committed state).
3. Ten DAA later `activate_due_classes` grants P1 against a table that already holds it: `{P1:1,
P2:1, P3…, q36:199, q25:199, base:598}` = **999‰**. The set check now passes again but the sum
check does not, and no code path ever returns the lost permille.

Net: for two ordinary transaction fees and one minimum bond, the network commits a state its own
loader declares invalid, permanently. Live nodes keep producing, but no new node can ever complete
`IBD with headers proof` again — and the standing policy in this repo forbids re-genesis, so there
is no remedy short of a ruleset change plus a re-mint. Repeating the pair walks BASE-0's share
down (measured: 600 → 520 over 40 cycles) until registrations are refused at the reserve.

### S-02 `[critical]` M2-3 is recorded fixed but only its pwu leg was closed: the coinbase still pays merged work the transition refuses for epoch budget or exposure, and the anti-replay still keys on a claim that was never created
`consensus/src/pipeline/virtual_processor/processor.rs:4119`  ·  verifier lenses: high, high

**Recorded as fixed.** M2-3, recorded `fixed` in docs/palw-mainnet-audit-2026-08-28.md remediation table ("the payment
path derives pwu exactly as the transition does"), and not revisited by docs/palw-mainnet-
reaudit-2026-08-29.md.

**Mechanism.** The 2026-08-28 audit's M2-3 named three predicates the payment path was missing against the
transition — `PwuClaimNotDerived`, `EpochBudgetExceeded`, `ExposureCeilingExceeded` — and
prescribed "make `palw_v2_unentitled_blues` ask the FULL `check_palw_attempt_admission_v2` ... and
back the anti-replay with a durable seen-attempt-id set that survives `retire_claim` instead of
claim presence". Commit f0a0e3e9 added ONLY the pwu equality (processor.rs:4124-4145), and the
remediation table records `M2-3 | fixed | the payment path derives pwu exactly as the transition
does`. The other two legs and the anti-replay key are byte-for-byte unchanged.

The two predicates are computed from the SAME parent state (`palw_state` at
`selected_parent(current)`, processor.rs:1205-1216), so the difference is purely the missing
checks. Payment asks four questions and stops: `check_palw_producer_entitlement_v2` (items 1-5
only — its own doc says the resource items are "excluded"), then `let attempt_id =
attempt_id_v2(&envelope.attempt); if state.claim(&attempt_id).is_some() ||
!seen_here.insert(attempt_id) { ... unentitled.insert(*blue); }` (processor.rs:4118-4122), then
the class lottery, then pwu. The in-tree comment says so out loud: "The epoch budget and the
exposure ceiling are sequential ... and belong with the wider fix noted in the audit, not here."

Accountability asks the full list. `palw_v2_merged_works` runs
`check_palw_attempt_admission_full_v2` per merged block (processor.rs:4941-4947) and pushes a
failure onto `skips`; step 4b of `apply_palw_transition_v4` re-runs
`check_palw_attempt_admission_v2` against the live fold state and on `Err(refused)` does
`builder.restore(checkpoint); Some(refused.to_string())` → `merged_skips.push(...)`
(palw_state_v2.rs:3386-3428). Both refuse on `if would_produce > budget { return
Err(EpochBudgetExceeded ...) }` (palw_admission_v2.rs:275-297) and on `if would_reserve > ceiling
{ return Err(ExposureCeilingExceeded ...) }` (palw_admission_v2.rs:305-336). `merged_skips` is
consumed only by an `info!` line (processor.rs:1420-1423); it never reaches the coinbase.

The coinbase then pays the block in full. Blues: `if palw_unentitled_blues.contains(blue) {
continue; }` is the only gate (coinbase.rs:210). Reds — which at `ghostdag_k = 1` is every block
of every class slower than the floor, by ADR-0058's own argument — are paid to their own script by
`if palw_pay_entitled_reds_to_their_miner && !mergeset_non_daa.contains(red) { ...
outputs.push(TransactionOutput::new(value, reward_data.script_public_key.clone())); } continue;`
(coinbase.rs:270-285), with `palw_pay_entitled_reds_to_their_miner =
self.palw_state_params_v2.is_some()` (utxo_validation.rs:1006), i.e. true on every V2 network.

So a merged block can be paid its whole §F worker carve while creating no claim, reserving no
exposure, incurring no panel duty, and entering no epoch counter — and because `apply_attempt`
never ran, `state.claim(&attempt_id)` stays `None`, so the anti-replay at processor.rs:4119 never
engages for that identity in any later block either. This is verbatim the mechanism M2-3
described; only its trigger moved from "inflated pwu on the budget-exempt floor class" to "budget
or exposure on any non-floor class".

**Failure scenario.** Shipped RC bundle (`palw_fp_devnet_v3.rs`): `EPOCH_LENGTH = 1_000`, `BUDGET_TOLERANCE_PERMILLE =
1_000` (exactly unity — no headroom), `min_grantable_share_permille() = ceil(1e6/(1000*1000)) =
1`. An attacker registers one class Q at the 1‰ grant floor under one Active bond.
`derive_epoch_budgets_v2` gives Q `budget_blocks = max(1, 1000*1*1000/(1000*denom)) = 1` block per
1000-DAA epoch.

Epoch e: the attacker produces ONE honest Q attempt. It is admitted, `apply_attempt` sets
`epoch_counters[Q].produced_blocks = 1`, and the class's budget is now full for the next ~999 DAA
(about 33 h at the frozen 120 s cadence).

Every further Q block in epoch e: the attacker performs NO inference at all — algo-6's Layer-0 tag
is `l1_tag_v2(commitment_root)`, a pure hash of the attempt (palw_attempt_v2.rs:322-333), and the
class ticket is `H(commitment_root)`, so `trace_root`, `output_root` and `execution_root` can be
arbitrary bytes. It grinds the network target and the class ticket, signs, and publishes. The
header passes `pre_ghostdag_validation` (shape, challenge binding, valid ML-DSA signature). The
block lands in a chain block's mergeset as an in-DAA-window red. `palw_v2_unentitled_blues` finds
the bond Active, the class Active and unfrozen, the artifact root correct,
`state.claim(attempt_id)` absent, `seen_here` fresh, `class_ticket <= target`, and `pwu ==
palw_pwu_v1(target, pwu_per_inference)` — NOT unentitled → coinbase.rs:283 pays the full worker
carve to the attacker's own script. `palw_v2_merged_works` and step 4b both refuse it with
`EpochBudgetExceeded { produced: 1, claimed: 1, budget: 1 }` → skip. No claim, no
`reserved_exposure`, no panel bound, no court reachable, nothing slashable — the fabricated roots
are never examined by anyone.

Amplification, unchanged from M2-3: `pre_pow_hash_64` passes `PalwCommitmentDigestRule::Exclude`
(hashing/header.rs:200-203) while `hash()` passes `Include` (hashing/header.rs:165), and ML-DSA
signing takes caller-supplied randomness, so the bond holder re-signs one solve into N headers
identical except `palw_commitment`. Held back and released one per chain block, each is merged by
a different chain block, so `seen_here` (per-block) never sees a duplicate and
`state.claim(attempt_id)` is still `None` — every one is paid. `merge_depth =
MERGE_DEPTH_DURATION/120 = 30` bounds N at ~30 full worker carves per solved header.

Secondary damage, all deterministic: because `epoch_counters[Q]` stops at 1,
`apply_class_retargets` measures Q as having produced one block against its expectation and EASES
its class target (making the next solve cheaper), and `apply_class_share_growth` reads "produced
every block its budget allowed" and WALKS Q's share up by `CLASS_GROWTH_PERMILLE = 250` at the
boundary. The attacker's class is rewarded, in difficulty and in cadence share, for the epoch it
spent being paid without claiming.

Exposure is a second, independent trigger for the identical outcome and needs no budget at all:
once `reserved_exposure(bond) + registration_exposure(bond) + pwu*slash_value >
collateral*500/1000`, every further attempt of that bond is `ExposureCeilingExceeded` → skipped →
paid. That path also fires on honest producers with no attacker present.

### S-03 `[critical]` M2-4's role-split capture makes every challenger's terminal close inadmissible: the court can convict no fraud at all, and the honest challenger is slashed for trying
`kaspad/src/palw_panel.rs:1636`  ·  verifier lenses: critical, critical

**Recorded as fixed.** M2-4, recorded `fixed` in docs/palw-mainnet-audit-2026-08-28.md:115 — "capture chosen by role
(challenger bisects its OWN execution); the rung clock does not run at `Terminal`"; the 2026-08-29
re-audit calls the mechanism "genuinely repaired" (line 458) while noting its acceptance round
trip is untested.

**Mechanism.** The M2-4 fix selects the capture by role for the WHOLE duty match, including the terminal arm. At
`palw_panel.rs:1636` the challenger's capture is unconditionally its own execution — `let
capture_from_own = (!duty.i_am_responder).then(|| own_executions.get(&duty.claim_id)).flatten();`
— and `own_executions` is guaranteed populated for a challenger, either at court-open
(`own_executions.insert(target.claim_id, mine_run.material.clone());`, :1452, taken only on the
`!reproduced` branch) or by the re-execution at :1614-1633 which otherwise `continue`s. That same
`capture` is then fed to the terminal move at `palw_panel.rs:1715`: `let refutation = match
backend.refutation_for_index(capture, index)`.

`refutation_for_index` derives the refutation's binding FROM the material it is handed: `misaka-
palw-base0/src/backend.rs:151` decodes `(binding, tiles, logits_rows, generated, _) =
base0_material_decode_v1(material)` and `misaka-palw-base0/src/legs.rs:721-722` carries that
decoded `binding` verbatim into `PalwExecutionStepRefutationV1 { binding, .. }`. So a challenger's
refutation carries the CHALLENGER's `full_logits_trace_root` and `committed_execution_root`.

The adjudicator pins the binding to the accused claim before it reads any evidence —
`consensus/core/src/palw_court_v2.rs:487-488`:
```rust
check_arithmetic_close_binding(claim.trace_root, binding_logits_root_of(&refutation.binding))?;
check_execution_root_binding(claim.execution_root, refutation.binding.committed_execution_root)?;
```
The challenger only reaches the terminal arm because those roots DIFFER (`let reproduced =
mine_run.execution_root == target.execution_root && mine_run.trace_root == target.trace_root;`,
:1440, and `own_executions` is written only when `!reproduced`). So the first check returns
`TraceRootMismatch`, `palw_court_close_verdict_v2_impl` (`processor.rs:3835`) ends in `.ok()` →
`None`, and the panel takes the `else` at `palw_panel.rs:1741`, counting `"the chain reads no
verdict from this close"` and never submitting. The responder is the fraudster and will not
convict itself. Nobody closes.

The backend trait's own doc states the invariant the fix breaks —
`consensus/core/src/palw_backend.rs:132-136`: "Returned by BOTH sides, and deliberately the same
call for both: an honest executor closing its own case and a challenger closing a real fraud
assemble the identical object." Identical objects require the same material; role-splitting the
capture at the terminal step makes them different. `palw_court_v2.rs:1136-1139` confirms which
material a conviction must come from: the test builds the claim as `trace_root =
refutation.binding.full_logits_trace_root; execution_root =
refutation.binding.committed_execution_root;` — i.e. the refutation is assembled from the
FRAUDULENT capture, and the fix now denies the challenger exactly that.

The correct shape is role-split for the rungs (`bisect_prefix_state`, :1667 and :1683 — those
genuinely need two executions) and the claim-matching capture for the terminal close.

**Failure scenario.** Producer P commits a claim on the floor class with one tampered step tile and a re-derived binding
(`execute_with_injected_fault`, backend.rs:103-140 — the seats' `verify_material` accepts it
because the roots are self-consistent). Honest node C runs the same job, gets different roots, and
opens a court; the ladder now diverges correctly and narrows over 22 rungs to the tampered leaf f
(~3,000 of the job's ~7,900 real leaves). At `Terminal`, C builds `refutation_for_index(own honest
capture, f)`; `adjudicate_court_close_v2` refuses it with `TraceRootMismatch` before reading a
byte of evidence, so `palw_court_close_verdict_v2` returns `None` and C files nothing. P files
nothing (a close from P's own capture would convict P). No rung clock runs at `Terminal`
(`court_next_deadline_v2`, palw_state_v2.rs:3556, and `turn_can_still_move` at :3606), so the
session sits until `opened + window_court` (3,000 DAA). The backstop then runs
`rearm_after_challenger_side_close` (palw_state_v2.rs:3675), which unconditionally executes
`builder.slash_seat(challenger_bond, claim.reserved, min_collateral_sompi)` (:3526) — C loses up
to `min_collateral_sompi` of collateral — and re-arms P's claim, which then finalizes and is paid
its escrowed reward. Net: proven-wrong arithmetic is unpunishable and unbounded; the only party
that can detect it pays to report it. Every additional honest challenger on the same claim pays
again.

### S-04 `[critical]` The panel material pool is capped per-claim and by age only — an unauthenticated peer grows it without limit; "bounded by count and age" is false for the count that matters (remote OOM)
`kaspad/src/palw_panel.rs:1327`  ·  verifier lenses: critical, critical

**Recorded as fixed.** M2-2 | fixed | "retention only for verified duty material, bounded by count and age, unicast serve
under a global budget, no lock across the disk read"

**Mechanism.** The inbox drain pools EVERY gossiped material with no verification, no chain-existence check and
no duty check:

```rust
let pool = materials.entry(claim).or_default();
if pool.len() >= MATERIALS_PER_CLAIM { pool.remove(0); }
pool.push(bytes);
```
(:1319-1327 inside `while let Ok(event) = inbox.try_recv()` at :1303). `materials: HashMap<Hash64,
Vec<Vec<u8>>>` (:1239). The caps are per claim (MATERIALS_PER_CLAIM = 4, :69) and per payload
(PALW_MATERIAL_MAX_BYTES = 16 MiB, palw_gossip.rs:44). There is no cap on the number of claim keys
and none on total bytes — `materials.len()` is never read anywhere in the file, and the only
`materials.retain` is the age one at :2207:

```rust
let stale = |claim| first_seen.get(claim).map(|seen| current_daa >
seen.saturating_add(PANEL_POOL_RETENTION_DAA)).unwrap_or(true);
materials.retain(|claim, _| live.contains(claim) || (!submitted.contains_key(claim) &&
!stale(claim)));
```
with PANEL_POOL_RETENTION_DAA = 4_000 (:103). The M2-2 fix's own new line at :1362-1363 stamps
`first_seen` for every pooled claim, so the `unwrap_or(true)` default it added can never fire for
a pool entry: every foreign claim is held for a full 4,000 DAA = 4,000 x 120 s = 5.6 days at the
frozen cadence (palw_mode_v2.rs:50), ~1.4 days if the DAA score advances 4 per block. The claim id
is 64 raw bytes off the wire with no authentication — the gossip module says so itself at
palw_gossip.rs:139-144 — so a peer names a fresh one per message.

Arithmetic: 4 x 16 MiB = 64 MiB of pooled RAM per claim id, unlimited claim ids, held 1.4-5.6
days. The only throughput brake is INBOX_CAP = 256 (palw_gossip.rs:56) drained on a 2 s tick
(:1297), i.e. >=128 admitted materials/s, so the pool absorbs the attacker's whole link up to ~2
GiB/s.

**Failure scenario.** An attacker opens one TCP connection to any node running `--palw-panel` (has_consumer() is true
there, so every Fresh material is copied into its inbox at palw_gossip.rs:309-311) and sends
PalwTraceMaterialBroadcast messages with fresh random 64-byte claim ids and 16 MiB payloads. Each
is Fresh (unspent per-claim budget), is relayed to every other peer, and is pooled forever-
for-4,000-DAA. At a 10 Mbps uplink (1.25 MB/s of pooled bytes) a 32 GiB host is exhausted in 32
GiB / 1.25 MB/s = 7.3 hours; at 1 Gbps, in 4.4 minutes. Cost: one connection, no bond, no block,
no transaction. Every seat on the network can be killed at once, and with the seats dead no claim
gathers a quorum, so every producer's escrowed worker carve is destroyed at its receipt deadline.
The same bound also fails to clear the OOM the code itself measured: the comment at :2185-2192
records the pre-fix leak at "RSS climbed from zero to 11 GB in twelve hours and was OOM-killed
roughly every thirty [hours]" on HONEST testnet-11 traffic; 22 GB/day x the new 1.4-5.6-day window
is 31-122 GB steady state, still above the ~27 GB RSS that was already killing the process.

### S-05 `[high]` Scheduling the VLT shadow fork still partitions the network at deploy time: the fence is normalised out of the identity, the model table it is forced to move with is not
`consensus/core/src/config/params.rs:1024`  ·  verifier lenses: high, high

**Recorded as fixed.** R-2, recorded "fixed": "one exhaustive for_each_fence drives BOTH ids... dns.vlt.*, dns.tkn.* ...
are now covered". Its own failure statement — "Scheduling it still partitions the network at
deploy time — the defect M1-6 exists to remove, still present for the upgrade M1-6 was written
for" — is still true after the fix.

**Mechanism.** `consensus_identity_id` (params.rs:819-823) normalises only the values `for_each_fence` visits:

```rust
let mut normalized = self.clone();
normalized.for_each_fence(&mut |score| *score = if *score == 0 { 0 } else { u64::MAX });
normalized.consensus_params_id()
```

Inside the `VltParams` destructure, the model table is bound and ignored (params.rs:1024):
`model_cost_table: _,` — correctly, it is not a fence. But `consensus_params_id` hashes the whole
overlay as one borsh blob (params.rs:1170): `let bytes = borsh::to_vec(dns).expect("DnsParams is
borsh-serializable");`, and `ModelCostTable` is a fixed-capacity `BorshSerialize` field of
`VltParams` (vlt.rs:1527-1531). So the table is inside the identity.

The table cannot stay put when the fence moves. `with_registered_models` (params.rs:1544-1552),
which wraps BOTH `From<NetworkType>` and `From<NetworkId>` and is therefore what every node
actually runs (`kaspad/src/daemon.rs:428`, `let params: Params = network.into();`), fills it as a
direct function of the fence:

```rust
if let Some(dns) = params.dns_params.as_mut()
    && dns.vlt.vlt_shadow_activation_daa_score != u64::MAX
    && dns.vlt.model_cost_table.len == 0
{
    dns.vlt.model_cost_table = crate::vlt::ModelCostTable::palw_metal_registered();
}
```

`palw_metal_registered()` sets `table.len = 4` and four non-zero entries (vlt.rs:1584-1588);
mainnet ships `vlt: VltParams::INERT` whose table is `ModelCostTable::EMPTY` (`len: 0`, all-zero
entries — vlt.rs:1542, 1867). The file itself says so: "a node's params — and therefore its
`consensus_params_id` — carry the model cost table" (params.rs:2703-2704), and "The model table
rides along automatically" (params.rs:2139-2140). The documented fork edit spells the table out as
part of the same release (params.rs:2013-2018), and
`shipped_presets_are_either_dormant_or_fully_forkable` (dns_finality.rs:9360-9398) fails the build
on the MATERIALIZED presets if a fence is moved over an empty table: `"{name}: fences moved with
an empty model table"` (dns_finality.rs:9385-9388).

So the normalised params of the two builds differ by `len` plus four `ModelCostEntry` values, the
borsh blobs differ, and `consensus_identity_id` differs. At
`protocol/flows/src/flow_context.rs:1405-1416` that is the refusal branch, not the warning branch:

```rust
let identity_agrees =
    !peer_version.consensus_identity_id.is_empty() && peer_version.consensus_identity_id ==
local_identity.as_bytes();
if !identity_agrees {
    return Err(ProtocolError::WrongConsensusParams(...));
}
```

The R-2 regression test misses it because it never materialises.
`scheduling_any_fence_leaves_the_identity_alone` builds its VLT case from the raw const
(params.rs:5184-5186): `let mut p = MAINNET_PARAMS;
p.dns_params.as_mut().unwrap().vlt.vlt_shadow_activation_daa_score = H;` —
`with_registered_models` never runs, both sides keep the empty table, and the assertion passes.
This is the exact trap `shipped_presets_have_pinned_fingerprints` records two hundred lines below
it (params.rs:5498-5504: "MATERIALIZED, not the raw const... caught live, where a correctly-built
release announced 5fabb683… while this test was green on 62e299b6…").

**Failure scenario.** Mainnet is live on this build. The team cuts the release that ADR-0024 step 3 calls for and that
params.rs:2010-2018 documents as the next mainnet fork: `PRODUCTION_DNS_PARAMS.vlt` becomes
`VltParams { vlt_shadow_activation_daa_score: H, vlt_activation_daa_score: u64::MAX,
..VltParams::INERT }` with H a coordinated FUTURE DAA score — exactly the shape
`TESTNET_DNS_PARAMS` already uses (params.rs:2254-2259). Operator A upgrades; operators B..Z have
not. A's node materialises through `From<NetworkId>`, `with_registered_models` fires (fence !=
u64::MAX, table len 0) and installs the four registered profiles; B's node keeps the empty table.
A's `consensus_identity_id` != B's. Both sides hit `flow_context.rs:1410` and return
`ProtocolError::WrongConsensusParams` — A cannot connect to a single un-upgraded peer, and no un-
upgraded peer can connect to A. As each operator upgrades the mesh splits into two disjoint sets
that cannot exchange blocks, each building its own chain on identical rules (H is still in the
future, so nothing about the rules differs yet), until the last operator crosses over. That is the
deploy-time partition M1-6 was written to remove and R-2 claims to have removed, on the one fence
this file calls "the ONE constant a release cut has to choose". The same applies to `dns.tkn.*`:
the TKN fences cannot be armed without also freezing `emission_epoch_budget_r0_atomic` and the
rest of `TokenParams::INERT`'s TBD numbers (params.rs:2034-2036), which are non-fence fields and
therefore in the identity too.

### S-06 `[high]` Two of three genesis classes still have no court responder, and M2-5's fix now makes accusing them a guaranteed loss for the accuser at 60 DAA
`consensus/core/src/palw_state_v2.rs:3638`  ·  verifier lenses: high, high

**Recorded as fixed.** M2-5, recorded `fixed` in docs/palw-mainnet-audit-2026-08-28.md:116 — "opening-rung silence closes
challenger-side; the inverted test is restored to the guard". The original M2-5 had two halves;
only the conviction-on-silence half was addressed, and the remediation table records the finding
as closed without qualification.

**Mechanism.** `Qwen36Backend` (`misaka-palw-base0/src/qwen36_backend.rs:273`) and `Qwen25A16Backend`
(`qwen25_a16_backend.rs:197`) implement only `model_id`, `job_for_anchor`, `execute` and
`verify_material`; they take the trait defaults `fn bisect_prefix_state(...) -> Option<Hash64> {
None }` and `fn refutation_for_index(...) -> Err(...)` (`consensus/core/src/palw_backend.rs:128`,
`:140`), and their own test pins it (`qwen36_backend.rs:444`,
`the_court_methods_are_honestly_unavailable`). The panel's disclosure arm dead-ends on exactly
that: `let Some(mid_state) = backend.bisect_prefix_state(capture, midpoint) else {
*court_stalls.entry("the backend cannot state its prefix at the midpoint")... continue; }`
(`palw_panel.rs:1667-1670`).

Nothing gates `CourtOpened` on class adjudicability: `validate_court_opened_v2`
(`palw_court_v2.rs:176-226`) checks phase, window, bond status, self-challenge, space size and the
challenger's signature — no class check — and the transition arm (`palw_state_v2.rs:4733-4840`)
adds none.

The M2-5 fix changed WHO pays for that silence, not whether it happens. `sweep_court_deadlines` at
`palw_state_v2.rs:3638`:
```rust
crate::palw_bisect::PalwBisectPartyV1::Responder if session.ladder.round() == 0 => {
    rearm_after_challenger_side_close(builder, ctx, session.claim, &claim,
session.challenger_bond)?;
}
```
and `rearm_after_challenger_side_close` begins with `builder.slash_seat(challenger_bond,
claim.reserved, builder.params.min_collateral_sompi())?;` (`:3526`). The arm's own comment says
round 0 "falls through to the session backstop", but the code charges immediately at the rung
deadline — `first_deadline_daa = ctx.daa_score + turn_deadline_daa()` = opened + 60 on the RC
bundle (`palw_state_v2.rs:4789-4793`; `COURT_TURN_DEADLINE = 60`, `palw_fp_devnet_v3.rs:130`).

So for a class with no `bisect_prefix_state`, round-0 responder silence is not a possibility but a
certainty, and its consensus consequence is a slash of the accuser. The two classes are not
fringe: `PALW_RC_GENESIS_QWEN36_SHARE_PERMILLE = 200` and
`PALW_RC_GENESIS_QWEN25_A16_SHARE_PERMILLE = 200` (`consensus/core/src/config/params.rs:2767`,
`:2772`) — 40% of genesis cadence.

**Failure scenario.** Producer P mines a Qwen3.6-class block with a garbage execution and an honestly re-derived
binding. Every seat's `verify_material` passes (it only checks that the capture reproduces the
claim's committed roots and answers the block's anchor), so the claim licenses. Honest node C,
which holds the 33 GiB Qwen3.6 artifact, re-executes at `palw_panel.rs:1439`, sees different
roots, and opens a court — consensus accepts it, because no rule asks whether the class has a
responder. P's own panel (or any honest node acting for P) resolves `Qwen36Backend`, calls
`bisect_prefix_state`, gets `None`, and is silent because no code in this tree can answer. Exactly
60 DAA later `sweep_court_deadlines` fires the opening rung, `declare_no_show` names the Responder
at round 0, and C is slashed `min(claim.reserved, min_collateral_sompi)` while P's claim is re-
armed and finalized. Repeating the accusation costs C another slash each time; there is no
sequence of moves by which C can ever win, because the ladder cannot leave round 0 and
`refutation_for_index` is `Err` for the family in any case. Result: arithmetic fraud in 40% of
genesis cadence is unpunishable, and the honest detector is the only party charged.

### S-07 `[high]` Every merged free-prompt receipt block (algo-7) is denied its entire coinbase reward, subsidy and fees, while the chain folds its work and mints weight for it
`consensus/src/pipeline/virtual_processor/processor.rs:4085`  ·  verifier lenses: medium, high

**Mechanism.** `palw_v2_unentitled_blues` disqualifies every mergeset member that is not an attempt-lane block:

```rust
// A non-attempt-lane block on a V2 network is not a block this chain accepted work
// from. The header gate already refuses the algorithm, so this is the same fact read
// a second time rather than a new rule.
if header.pow_algo_id != kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2 {
    unentitled.insert(*blue);
    continue;
}
```
(processor.rs:4082-4088)

The justifying comment is false. A `ConsensusV2` network accepts TWO ids:
`pre_ghostdag_validation.rs:105-113` calls `check_algo_id_for_mode_accepting(header.pow_algo_id,
..., self.palw_consensus_mode.accepts_algo_id(header.pow_algo_id), ...)`, and
`PalwConsensusParamsV2::accepts_algo_id` is `algo_id == self.algorithm_id || algo_id ==
self.freeprompt.receipt_algorithm_id()` (palw_mode_v2.rs:713-715), i.e. 6 OR 7.
`check_algo_id_for_mode_accepting`'s own doc names this exact mistake: "a gate comparing against
`required_algo_id` alone refused every block on the second [lane] ... Conflating them is what left
the receipt lane unenterable" (pow_layer0.rs:646-657). That conflation was fixed at the header
gate and reintroduced here at the payment gate.

Everything else in the pipeline treats algo-7 as first-class. `palw_v2_merged_works` folds it:
`Ok(None) => match self.palw_v2_check_receipt_spend(...) { Ok(Some(envelope)) =>
works.push(PalwMergedOwnedWorkV1::Spend(*blue, envelope)), ...}` (processor.rs:4948-4953). Step 4b
applies it, and `apply_receipt_spend` credits `builder.state.safe_weight += per_quantum` and bumps
`receipt_epoch_counters` (palw_state_v2.rs:5018-5043) — but writes no payout and no escrow, so the
coinbase is the receipt block's ONLY compensation. ADR-0044 states the intended rule directly:
"the receipt block's coinbase needs **no escrow ladder** ... a receipt block cannot [be voided].
Ordinary coinbase maturity applies" (docs/adr/0044-palw-free-prompt-receipts.md:313-315) — i.e.
paid in full, unescrowed, not paid nothing.

The denial is total, not partial: the set is consumed by the blues loop (`coinbase.rs:210`), the
reds loop (`coinbase.rs:257`) — both `continue` BEFORE the reward lookup, so subsidy and both fee
classes are dropped — and by `coinbase_validator_pool` (coinbase.rs:365, 375), so the §E validator
share of that block is not minted either. The only escape is `if *blue ==
ghostdag_data.selected_parent { continue; }` (processor.rs:4074): a receipt block is paid if and
only if it wins the chain race.

**Failure scenario.** Shipped RC/testnet-11 bundle: `ATTEMPT_SHARE_PERMILLE = 900`, so `fp_attempt_share_permille = 900`
and the receipt lane holds 100‰ of cadence — `PalwConsensusParamsV2::validate` requires `0 < split
< 1000` precisely because the receipt lane is live. `palw_freeprompt_params_v3` is
`Some(bundle.freeprompt)` on every V2 network (processor.rs:683-686).

An honest free-prompt producer serves a user prompt, its claim reaches `Final`, and it mines an
algo-7 receipt-spend block B spending quantum i. B is valid at every gate and is relayed. Two
outcomes, decided by nothing the producer controls:

- B is the selected parent of the next chain block → `palw_v2_unentitled_blues` skips it at
processor.rs:4074 → paid its full worker share.
- B is a mergeset blue or red of the accepting chain block — which at `ghostdag_k = 1` is what
happens to any block whose anticone holds another block, i.e. any receipt block produced
concurrently with an attempt block, which is the normal case for a lane holding 10% of cadence
beside a lane holding 90% → processor.rs:4085 marks it unentitled → the blues loop or the reds
loop `continue`s past it → the coinbase carries NO output for B at all. Its subsidy is not minted,
its transaction fees are not minted, and its §E validator share is not minted. The transition
still spends the quantum irreversibly (`ledger.insert(spend.quantum_index)`, palw_state_v2.rs:5023
— `QuantumAlreadySpent` forever after) and still adds `per_quantum` to `safe_weight`.

So the free-prompt lane hands its work and its quantum to the chain and is paid only when it
happens to win the GHOSTDAG race, with no compensation, no retry and no diagnostic — the producer
sees a valid, accepted, weight-bearing block with an empty reward. Systematically, the designed
100‰ receipt lane earns materially less than its share of issuance, and the shortfall is burned by
don't-mint rather than redistributed.

### S-08 `[high]` The wallet mirrors only the DNS half of `BondSpendFilter::locks` — a PALW V2 bond's collateral is never excluded, and every `wallet send` from a producer's address silently never lands
`misaka-cli/src/wallet.rs:124`  ·  verifier lenses: high, high

**Recorded as fixed.** M1-3, recorded 'fixed' in docs/palw-mainnet-audit-2026-08-28.md:108 ('the wallet marks bonded
outpoints from the node's own registry and both spenders skip them'), and R-4, recorded 'fixed' in
docs/palw-mainnet-reaudit-2026-08-29.md:70 ('the wallet mirrors PalwSpendLocks::locks instead of
the set of all bonds'). The wallet's own doc comment at misaka-cli/src/wallet.rs:116 names
`PalwSpendLocks::locks` as what it mirrors; the predicate it actually reproduces is only the
`bond_view` branch of `BondSpendFilter::locks`.

**Mechanism.** Consensus's lock predicate has TWO branches
(`consensus/src/pipeline/virtual_processor/utxo_validation.rs:241-251`):

```rust
fn locks(&self, outpoint: &TransactionOutpoint) -> bool {
    if self.palw_locked.contains(outpoint) {
        return true;
    }
    self.bond_view.is_some_and(|view| { view.get(outpoint).is_some_and(|bond| { … }) })
}
```

`palw_locked` is `ctx.palw_v2_locked_bonds`, built from the PALW V2 registry by
`palw_v2_locked_bond_outpoints` (`processor.rs:3980-3998`) via `palw_bond_collateral_is_locked_v2`
(`consensus/core/src/palw_state_v2.rs:852-861`, `Active => true`, `Retiring{since_daa} => now_daa
< since_daa + withdrawal_delay`). It is armed independently of the DNS half at
`utxo_validation.rs:352` (`bond_gate_view.is_some() || !ctx.palw_v2_locked_bonds.is_empty() ||
…`).

The wallet's `locked_bond_outpoints` queries exactly one thing (`misaka-
cli/src/wallet.rs:130-141`):

```rust
.get_stake_bonds(kaspa_rpc_core::GetStakeBondsRequest { owner_pubkey_hash: None, status_in: None,
cursor: …, limit: 1000, pov_daa_score: None })
```

and that RPC reads only the DNS overlay store (`consensus/src/consensus/mod.rs:1315-1324`): `if
self.config.params.dns_params.is_none() { return StakeBondPage::default(); } …
self.storage.stake_bonds_store.read().iterator()…`. `palw_state_v2` is never consulted, and no RPC
in this build enumerates PALW bonds or answers "is this outpoint locked collateral" —
`get_palw_producer_facts` requires you to already know the outpoint and returns no retiring/lock
status (`rpc/core/src/model/message.rs:2223-2231, 2286-2296`). So the wallet's `bonds` set is
structurally missing every PALW bond, `let bonded = bonds.contains(&outpoint)` (`wallet.rs:222`)
is `false` for the collateral, and both `filter(|u| u.mature && !u.bonded)` guards
(`wallet.rs:344`, `wallet.rs:466`) pass it through.

That collateral is at the wallet's own address by construction:
`palw_bond_registration_binds_its_carrier_v2` requires `output.script_public_key ==
p2pkh_mldsa87_spk(&payout_payload)` (`consensus/core/src/palw_lifecycle_objects_v2.rs:217-227`),
and `pay_payee()` takes that payload from `--palw-producer-pay-address`
(`kaspad/src/palw_panel.rs:430-444`), which `docs/testnet11-join-mining.md:59-70` tells the
operator to produce with `misaka key address --key-file ~/.misaka/miner.seed` — the same key file
`misaka wallet send --key-file` spends from. It is non-coinbase, so `is_spendable_settled` returns
`true` unconditionally (`kaspa-pq-validator-core/src/lib.rs:1813-1815`) and it is always `mature`.

The filter runs only at acceptance (`validate_transaction_in_utxo_context`,
`utxo_validation.rs:1948-1952` -> `TxRuleError::SpendsNonReleasableBond`), reached through the
caller's `.filter_map(|(i, tx)| self…(…).ok().map(…))` (`:1906`) — i.e. a SKIP, not a block
rejection. Nothing in `mining/src/mempool/` consults it (grepping `bond`/`palw` there finds only
the attestation shard index and a mass constant), so the transaction is admitted, relayed, mined,
and then dropped at acceptance; `handle_new_block_transactions` removes it from the mempool anyway
(`mining/src/manager.rs:1129`, whose own TODO reads "should use tx acceptance data to verify that
new block txs are actually accepted").

The same blind spot covers the second PALW obligation the wallet cannot see:
`palw_burns`/`burn_owed` (`utxo_validation.rs:1953-1968`) rejects a spend of a released-but-
slashed bond that does not leave `slashed` sompi unclaimed.

**Failure scenario.** A testnet-11 producer follows `docs/testnet11-join-mining.md` §3: `kaspad --testnet --netsuffix=11
--palw-register-bond --palw-producer-key=~/.misaka/miner.seed --palw-producer-pay-address=<addr>`.
`Params::from(testnet-11)` = `palw_rc_shipped_params()` (`config/params.rs:1573`), a `ConsensusV2`
network, so `palw_state` is `Some` and `palw_v2_locked_bonds` is non-empty on every block. The
carrier locks the collateral (sized by `size_bond_collateral` to hold one claim, typically the
largest output at that address) in output 0 paying to `<addr>`; mining rewards and the fee change
accumulate at the same `<addr>`. The operator then moves rewards out: `misaka wallet send --key-
file ~/.misaka/miner.seed --to misakatest:q… --amount 100 --yes`. `locked_bond_outpoints` returns
the DNS-only set (empty of this bond), the collateral is marked `bonded: false` and `mature:
true`, largest-first sorting (`wallet.rs:467`) puts it at input 0, `submit_transaction` succeeds
and the CLI prints `{"ok": true, … "txid": "<id>"}` with `Mode: SUBMIT`. The transaction is mined;
at `calculate_utxo_state` `locks()` returns true through `palw_locked`, the tx is silently dropped
from `validated_transactions`, and the mempool evicts it when the block arrives. The recipient is
never paid, the operator's UTXOs are all still there, and every retry — and every `misaka wallet
utxo consolidate` chunk that happens to contain the collateral — reproduces it identically with
the same false success line. There is no error message anywhere in the operator's path.

### S-09 `[high]` describe_fingerprint expands an unbounded peer-supplied blob into a multi-gigabyte String during the handshake, before the peer is authenticated
`protocol/flows/src/flow_context.rs:359`  ·  verifier lenses: high, high

**Mechanism.** ```rust
fn describe_fingerprint(bytes: &[u8]) -> String {
    if bytes.is_empty() { "absent (peer predates this field)".to_owned() } else {
bytes.iter().map(|b| format!("{b:02x}")).collect() }
}
```
(flow_context.rs:359-361)

It is called on three peer-controlled `bytes` fields inside `initialize_connection`:
`describe_fingerprint(&peer_version.genesis_hash)` (flow_context.rs:1385),
`describe_fingerprint(&peer_version.consensus_params_id)` (1413) and, new in this changeset,
`describe_fingerprint(&peer_version.consensus_schedule_id)` (1422). None of them is length-checked
anywhere: `TryFrom<protowire::VersionMessage> for Version` copies `msg.genesis_hash`,
`msg.consensus_params_id`, `msg.consensus_identity_id`, `msg.consensus_schedule_id` straight
through with no bound (p2p/src/convert/messages.rs:49-68), and the transport accepts messages up
to `const P2P_MAX_MESSAGE_SIZE: usize = 1024 * 1024 * 1024; // 1GB` on both the client and server
channels (p2p/src/core/connection_handler.rs:45, 78, 122). The proto declares them as plain
`bytes` with no cap (p2p.proto:195-198, 206-207). The expansion is 2 bytes of output per input
byte plus one heap allocation per byte from the inner `format!`, and it happens before the peer is
registered in the hub or given a flow.

**Failure scenario.** An attacker opens an inbound p2p connection to a mainnet node and sends a VersionMessage with the
correct `network` string and a 1 GiB `genesisHash` (or, on the new path, the victim's correct
`consensusIdentityId` plus a 1 GiB `consensusScheduleId`, which reaches the warn! at
flow_context.rs:1417-1424). The node decodes the 1 GiB message, fails the genesis comparison, and
calls describe_fingerprint, which performs ~10^9 small allocations and builds a ~2 GiB String to
put in the error it then logs. One connection costs the attacker one 1 GiB upload and costs the
node several GiB of RSS plus minutes of CPU; a handful of concurrent connections OOM-kills the
node. No credentials, no prior peering, no valid chain state required.

### S-10 `[high]` A sidecar-import refusal on the headers-proof IBD path is not retryable — it permanently quarantines the node, and the pruning point it refuses on is the peer's unvalidated chain-locator tail, not the one that was installed
`protocol/flows/src/ibd/flow.rs:1599`  ·  verifier lenses: high, high

**Recorded as fixed.** M1-2, recorded "fixed" in docs/palw-mainnet-audit-2026-08-28.md:107 ("both sidecar imports moved
before the destructive clear") and R-3, recorded "fixed" in docs/palw-mainnet-
reaudit-2026-08-29.md:69 ("no child on that chain ⇒ refuse, which is the posture both callers
already took"). The destructive half is genuinely closed. The recovery premise both fixes rest on
— stated in code at processor.rs:10629-10631 and 10787-10790 as "the IBD can be retried" — is
false on the `DownloadHeadersProof` call site, where a refusal lands after the commit barrier and
quarantines the node.

**Mechanism.** `pruning_point_witness_child` and both importers justify refusing on a `None` witness with "the
IBD can be retried": `import_pruning_point_palw_state` says "REFUSE rather than write on trust ...
the IBD can be retried once the child header is in hand" (processor.rs:10629-10631) and the
overlay twin repeats it verbatim (processor.rs:10787-10790). On two of the three call sites that
is true. On the `DownloadHeadersProof` path it is not, and the pruning point handed to that site
is peer-chosen.

The pruning point actually installed comes from the validated proof — `let proof_pruning_point =
proof[0].last().expect("was just ensured by validation").hash;` (flow.rs:1947), used for
`apply_pruning_proof` / `import_pruning_points` / `sync_headers` (flow.rs:1817). But after the
commit barrier the flow calls

```rust
self.ctx.mark_active_consensus_replaced();          // flow.rs:1584
session = self.ctx.consensus().session().await;
self.sync_new_utxo_set(&session, negotiation_output.syncer_pruning_point).await?;   //
flow.rs:1599
```

and `negotiation_output.syncer_pruning_point` is simply the last hash of the peer's chain-block
locator:

```rust
let mut syncer_pruning_point = *locator_hashes.last().unwrap();   // negotiate.rs:40
```

`get_syncer_chain_block_locator` validates nothing about the contents beyond `len() > 64`
(negotiate.rs:183-187), and nothing anywhere compares it to `proof_pruning_point` —
`determine_ibd_type`'s `None`-highest-known branch (flow.rs:1749-1771), which is the branch a
fresh node takes, never consults it at all.

So `sync_new_utxo_set` is run against a pruning point the peer chose, while the node holds a
different one. Both sidecars then fail, either of them sufficient:

* `sync_pruning_point_overlay_snapshot`: the server serves only `Some(s) if s.pruning_point == pp`
(v8/request_pruning_point_snapshots.rs:93-98), so the reply is `found: false`, and mainnet has
`dns_params: Some(PRODUCTION_DNS_PARAMS)` (params.rs:2341) so `overlay_active` is true →
`Err("peer cannot serve the pruning point overlay snapshot required for pruned IBD on this
network")` (flow.rs:2368-2372).
* Or, if the peer answers `found: true` with any bytes, the path goes straight through the audited
selector: `get_children(X)` for a hash the node has never seen returns `KeyNotFound`,
`.unwrap_or_default()` makes it empty, `pruning_point_witness_child` returns `None`, and
`import_pruning_point_overlay_snapshot` takes `if verified_against.is_none() { ... return
Err(PruningImportError::ImportedOverlayCommitmentMismatch(pruning_point, got, Hash64::default()))
}` (processor.rs:10786-10795). On a ConsensusV2 preset (testnet-11)
`request_pruning_point_palw_state` fires first and errors the same way (flow.rs:2492-2496).

Either error propagates out of `ibd()`, and because line 1584 already ran:

```rust
pub fn finish_ibd_after_failure(&self) -> bool {
    let replaced = self.active_consensus_replaced.swap(false, Ordering::SeqCst);
    if replaced { self.chain_participation().quarantine(); }   // flow_context.rs:1080-1082
```

`quarantine()` stores `QUARANTINED` and persists it (chain_participation.rs:365-372); the gate's
own test `quarantine_never_clears_on_its_own` (chain_participation.rs:942-955) pins that no retry,
review or restart lifts it, and only `--clear-quarantine` does (chain_participation.rs:424,
kaspad/src/daemon.rs:1065). The gate is enabled on exactly Mainnet and Testnet
(daemon.rs:1054-1058), and closed it makes `should_mine` return false and `is_synced` report false
forever (rule_engine.rs:148, 167-171), which is also what the external validator polls before it
will attest.

**Failure scenario.** Mainnet, victim = any node joining by `DownloadHeadersProof` (a fresh node: it knows none of the
syncer's locator hashes, so `highest_known_syncer_chain_hash` is `None` and the relay header
clears `blue_score >= hst.blue_score + pruning_depth`). Attacker = an ordinary fully-synced node
on the honest chain that answers `RequestIbdChainBlockLocator(None, None)` honestly except that it
replaces the LAST hash with any 32 bytes the victim does not have. Everything else it serves is
the genuine chain: the real proof validates, the real headers download,
`validate_staging_timestamps` passes (the victim's local tip is genesis),
`validate_staging_palw_order` returns early because mainnet is `PalwConsensusMode::Disabled`
(params.rs:2347), `authorize_commit` and `commit_if` succeed, and
`mark_active_consensus_replaced()` runs. Then `sync_new_utxo_set(&session, X)` requests the
overlay snapshot for the garbage hash X, the peer's snapshot is keyed to the real pruning point,
the reply is `found: false`, and the IBD returns `Err`. Outcome: the victim holds the correct,
fully committed chain, its utxoset is untouched (M1-2's destructive half really is fixed) and will
be filled in by the next ordinary `IbdType::Sync` — but `chain_participation` is latched
`Quarantined` in the meta DB. It never mines, never attests, and reports `is_synced=false` across
every restart until an operator adds `--clear-quarantine` to the unit file. One lie in one field
of one message, costing the attacker nothing, bricks every new mainnet node that happens to pick
it as its first IBD peer. The same outcome occurs with no attacker at all whenever an honest
peer's pruning point advances between its locator reply and its proof reply, since the locator
tail is then the old pruning point and the proof carries the new one.

### S-11 `[high]` A peer's normal "found: false" for a pruning-point sidecar permanently QUARANTINES a node that just committed a headers-proof IBD
`protocol/flows/src/ibd/flow.rs:1599`  ·  verifier lenses: high, high

**Recorded as fixed.** Re-audit R-3 (docs/palw-mainnet-reaudit-2026-08-29.md): "The destructive half of the fix is real
and holds ... so a verification failure no longer leaves a node with its utxoset deleted. That
part is closed" and "The node is no longer damaged (that is M1-2's other half)". The node is
damaged — worse than before: a deleted utxoset re-runs on the next IBD, a quarantine never clears.

**Mechanism.** In the `IbdType::DownloadHeadersProof` arm the staging consensus is swapped in and then marked:
`self.ctx.mark_active_consensus_replaced();` (flow.rs:1584), and only afterwards
`self.sync_new_utxo_set(&session, negotiation_output.syncer_pruning_point).await?;`
(flow.rs:1599). Every sidecar refusal inside that function is a `?`. The overlay one is `if
!msg.found { if overlay_active { return Err(ProtocolError::Other("peer cannot serve the pruning
point overlay snapshot required for pruned IBD on this network")) } }` (flow.rs:2369-2375), with
`overlay_active = self.ctx.config.dns_params.is_some()` — true on MAINNET_PARAMS (`dns_params:
Some(PRODUCTION_DNS_PARAMS)`, params.rs:2341). The EVM twin is the same shape (flow.rs:2325-2329)
and is armed on testnet/testnet-11 (`evm_activation_daa_score: 0`, params.rs:2507, inherited by
TESTNET11_PARAMS via `..TESTNET_PARAMS` at params.rs:2623).

The server answers `found: false` as a matter of course. It holds exactly ONE overlay snapshot and
serves it only on an exact match: `Some(s) if s.pruning_point == pp => ... , _ =>
PruningPointOverlaySnapshotMessage { found: false, overlay_snapshot: vec![] }`
(v8/request_pruning_point_snapshots.rs:93-97). The EVM serving side says so in its own comment:
"`None` if neither yields it (the peer tries another server)"
(virtual_processor/processor.rs:10520).

The requester does not try another server. The `?` unwinds to flow.rs:343, `if
self.ctx.finish_ibd_after_failure() {`, which reads the flag set at 1584 and calls
`self.chain_participation().quarantine();` (flow_context.rs:1079-1086). `quarantine()`
(core/src/chain_participation.rs:365-372) stores QUARANTINED and persists it; `with_persistence`
maps a restored `IbdRunning | Quarantined` back to QUARANTINED (chain_participation.rs:179);
`allows_participation()` is then false forever. The only exit is `operator_clear_quarantine`,
reached solely from the `--clear-quarantine` startup flag.

**Failure scenario.** Mainnet node N joins by headers-proof IBD from peer P, whose pruning point is PP0 at negotiation
time. N's header sync plus trusted-block processing takes several minutes; during that window P's
pruning point advances to PP1 and `capture_pruning_point_overlay_snapshot` overwrites P's single
stored snapshot with PP1's. N commits staging (mark_active_consensus_replaced), then asks P for
the overlay snapshot at PP0. P replies found:false. N returns Err out of sync_new_utxo_set,
finish_ibd_after_failure sees replaced==true, and N is QUARANTINED: it cannot mine, cannot attest,
and reports is_synced=false, across restarts, until an operator adds --clear-quarantine and
restarts. Retrying against another peer cannot help, because the gate is already latched. The same
happens on testnet-11 whenever a syncer cannot materialise the EVM snapshot for the requested
point (processor.rs:10520 documents that as an expected answer). A node whose validator seat goes
silent this way stops attesting, so enough simultaneous occurrences stall DNS finality.

### S-12 `[high]` A 560-byte-per-minute request loop spends a node's entire 64 MiB/60 s serve budget, so the pull that M2-1's fix depends on is remotely switchable off
`protocol/flows/src/palw_gossip.rs:209`  ·  verifier lenses: high, high

**Recorded as fixed.** M2-2 | fixed | "unicast serve under a global budget" (the budget is presented as the brake the
per-claim throttle was not)

**Mechanism.** The budget is global to the node and keyed on nothing — not the peer, not whether the claim is
live, not whether the requester has any standing:

```rust
let mut budget = self.serve_budget.lock().unwrap();
...
if budget.bytes_served.saturating_add(bytes.len() as u64) > SERVE_BUDGET_BYTES_PER_WINDOW { return
None; }
budget.bytes_served += bytes.len() as u64;
```
(:202-213) with SERVE_BUDGET_BYTES_PER_WINDOW = 64 MiB per SERVE_BUDGET_WINDOW = 60 s (:125-126).
The serve is unicast to the asker (v8/palw_gossip_flow.rs:78), so the requester is charged nothing
and keeps the bytes. At the shipped 9.7 MB material the budget is 64 MiB / 9.7 MB = 6.9 serves per
minute FOR THE WHOLE NODE, shared by every peer. The refusal is a silent `None` — no log, no
reject message, nothing an operator can see. `served_recently` (:181-188) is per claim and 10 s,
so it bounds repetition of one claim and nothing else; the requester chooses the claims.

**Failure scenario.** An attacker opens one inbound connection to the producer's node (or to any seat holding retained
material — the flow is registered for every peer unconditionally, v8/mod.rs:143-153) and sends 7
PalwMaterialRequest messages per minute naming 7 distinct claims the node retains. Claim ids are
public: PanelBound is on chain and attempt_id_v2 is computable from any block header. 7 x ~80
bytes = 560 bytes/minute. For the remaining ~59 s of every window, every honest seat's pull for
every claim is answered with silence. Composed with finding 1 this completes the slashing machine:
the seats cannot obtain the material, sign `Unavailable`, and 3 of 5 make `ProducerDefaulted`
(processor.rs:4404), slashing the honest producer's bond (palw_state_v2.rs:4897). The same number
also bites with no attacker at all: 5 seats pulling one 9.7 MB claim consume 48.5 MB, so two
claims pulled in the same minute exceed the node's entire budget — against a doc that sizes 64 MiB
as "~8 of the largest registered class's materials — enough to unstick a neighbourhood".

### S-13 `[high]` The serve reads the file before consulting the budget and never records a budget-refused claim, so an 80-byte request buys an unthrottled 16 MiB blocking read on a tokio worker
`protocol/flows/src/palw_gossip.rs:194`  ·  verifier lenses: high, high

**Recorded as fixed.** M2-2 | fixed | "unicast serve under a global budget, no lock across the disk read" — the lock half
landed; the prescribed "move the read to spawn_blocking" did not, and the ordering was not
considered

**Mechanism.** `resolve_material_for_serve` runs in this order: (1) 10 s per-claim throttle check (:181-189), (2)
clone the resolver Arc out of the mutex (:193), (3) **`let bytes = resolver(claim)?;`** (:194) — a
synchronous `std::fs::read` of up to 16 MiB, since the registered closure is
`std::fs::read(palw_retained_material_path(..)).ok().or_else(||
std::fs::read(retention.join("foreign")...).ok())` (palw_panel.rs:1232-1236), (4) size check
(:195), (5) budget check `return None` (:209-211), (6)
`self.served_recently.lock().unwrap().insert(claim, now)` (:214).

Two consequences from that ordering. First, the expensive step (3) happens before the cheap gate
(5), so the budget bounds egress but bounds neither disk I/O nor CPU. Second, because (6) is AFTER
(5), a claim refused by the budget is never recorded as served — so the 10-second throttle at
:184-188 never engages for it, and the identical request can be repeated immediately and
indefinitely for as long as the budget stays spent (which finding 3 shows costs 560 bytes/minute
to maintain).

And the read is executed inline in `PalwGossipFlow::start_impl` (v8/palw_gossip_flow.rs:73), which
is a `tokio::spawn`ed task (flow_trait.rs:19-20) on the runtime shared with block relay, IBD and
RPC — not `spawn_blocking`, which is precisely what M2-2's prescribed fix asked for ("clone the
Arc out of the mutex before calling it, and move the read to spawn_blocking").

**Failure scenario.** An attacker opens M connections to a seat, spends the node's serve budget with 7 requests (finding
3), then sends PalwMaterialRequest for ONE claim the node retains, at the rate the flow can
service. Each 80-byte request performs a full `std::fs::read` of the 9.7 MB retained material,
hits the spent budget, returns None, emits zero bytes, and leaves `served_recently` untouched so
the next request repeats it immediately. Per connection this pins one tokio worker in a blocking
read continuously; with M >= the runtime's worker count the shared runtime stalls, taking p2p
flows, block relay and RPC with it. Amplification: 80 bytes in, up to 16 MiB of read plus one
blocked reactor thread out (~2x10^5), with zero outbound traffic to make it visible to monitoring.

### S-14 `[high]` The solicited exemption has no count bound and the pool evicts the OLDEST, so an attacker that merely keeps sending during the 120 s pull window deletes the honest answer before the tick reads it
`protocol/flows/src/palw_gossip.rs:276`  ·  verifier lenses: high, high

**Recorded as fixed.** M2-1 | fixed | "pool evicts oldest, solicited answers exempt from the per-claim budget"

**Mechanism.** Within PULL_SOLICITED_TTL (120 s, :128) of `note_pull_request`, the per-claim budget is skipped
entirely for that claim — there is no separate cap on solicited admissions:

```rust
if *count >= PALW_MATERIALS_PER_CLAIM && !solicited { return PalwGossipAdmit::Duplicate; }
*count += 1;
```
(:276-279). So during the window an unbounded number of distinct payloads for that one claim are
`Fresh`: each is relayed to every peer and copied into the inbox. On the panel side each admitted
payload evicts the pool's oldest entry:

```rust
if pool.len() >= MATERIALS_PER_CLAIM { pool.remove(0); }
pool.push(bytes);
```
(palw_panel.rs:1324-1327). The inbox is drained to empty at the top of the tick (:1303) before any
duty is examined (:1770 onward), so what the verdict loop sees is the LAST four payloads admitted
before the tick fires. The fix's stated principle — "whoever is first must not be able to lock out
whoever is right" (:1322-1323) — is inverted into "whoever is last decides", and with the budget
switched off for that claim the attacker is always able to be last.

**Failure scenario.** Seat S pulls claim C. `request_palw_material` broadcasts the request to every peer with protocol
>= PALW_PULL (flow_context.rs:539-548), so the attacker sees it (and does not even need to:
sending continuously for the receipt window costs it nothing extra). The attacker sends 4 distinct
payloads for C every 100 ms. When the honest answer arrives it is admitted (solicited), pooled —
and then evicted within ~400 ms unless S's 2-second tick lands in that gap, which is ~5% of the
time. The pull throttle is 25 DAA (50 min at the frozen cadence, palw_panel.rs:1865), so over the
300-DAA half-window S gets ~12 attempts, each denied with ~95% probability. S then signs
`Unavailable` (:1876); three such receipts slash the honest producer (palw_state_v2.rs:4890-4897)
and also slash any seat that filed `Valid`. Secondary effect: for 120 s the per-claim relay cap is
off on every node that pulled, so each relays unbounded 16 MiB payloads for that one claim to all
of its peers.

### S-15 `[high]` M2-2's global serve budget lets ~1 KB/min of requests shut off the PALW material pull for a whole node
`protocol/flows/src/palw_gossip.rs:203`  ·  verifier lenses: high, high

**Recorded as fixed.** Audit M2-2, recorded fixed. The 2026-08-29 re-audit lists M2-1/M2-2 among the items it did not
verify ("the M2 transport surface ... are unverified here").

**Mechanism.** The M2-2 repair replaced the broadcast serve with a unicast plus a single process-wide byte
budget:

```rust
const SERVE_BUDGET_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);
const SERVE_BUDGET_BYTES_PER_WINDOW: u64 = 64 << 20;
...
if budget.bytes_served.saturating_add(bytes.len() as u64) > SERVE_BUDGET_BYTES_PER_WINDOW {
    return None;
}
budget.bytes_served += bytes.len() as u64;
```
(palw_gossip.rs:125-126, 203-213)

`serve_budget` is one `Mutex<ServeBudget>` on the single `PalwGossipCenter` held by FlowContext —
it is not per peer, and there is no per-peer request rate limit anywhere in the serving flow
(`v8/palw_gossip_flow.rs:63-79` decodes the claim id and calls `resolve_material_for_serve`
directly). The only other limiter is a 10-second per-claim window (`palw_gossip.rs:181-188`),
which bounds repetition of one claim to 6 serves per minute, not the total. Real materials are
~9-10 MB by this file's own sizing note ("QWEN25-A16's canonical job encodes to 9.7 MB",
palw_gossip.rs:37-38), so 6 repeats of a single claim id consume ~58 MB of the 64 MB window and
two claim ids exhaust it outright. Once exhausted, `resolve_material_for_serve` returns None for
every honest requester until the window rolls.

The cost of that denial is stated in this same file and in the panel: a seat with no verifying
material "signs `Unavailable`" at the half-window, "and an honest producer's bond is slashed"
(palw_gossip.rs:271-275, kaspad/src/palw_panel.rs:1855-1880).

**Failure scenario.** Attacker peers with the panel-running nodes of a testnet-11/mainnet fleet (8 nodes is one
connection each). It reads live claim ids off the gossip it already receives. Per node it sends 12
`PalwMaterialRequest` messages a minute (two claim ids, alternating every 10 s) — about 840 bytes
— and the node's 64 MiB serve budget is spent on the attacker within seconds of each window
opening. In the same minute the attacker sends 4 junk ~70-byte payloads for the target claim C to
every seat, which fills `PALW_MATERIALS_PER_CLAIM` (palw_gossip.rs:276) so the honest producer's
push for C is admitted as `Duplicate` and never reaches the seats' inboxes. The seats then pull
for C (panel.rs:1868-1870) — the solicited exemption would let the answer through, but no node has
serve budget left to send one. At the half-window every seat signs `Unavailable` for C, the quorum
defaults the claim, and the honest producer's bond is slashed. Total attacker cost: under 1 KB per
node per minute.

### S-16 `[medium]` A non-floor class's own attempt can never carry a chain block across an epoch boundary: admission reads the parent state's budget table, which is always the previous epoch's
`consensus/core/src/palw_admission_v2.rs:277`  ·  verifier lenses: medium, medium

**Mechanism.** Item 7 demands a budget record for the CANDIDATE's epoch out of the PARENT's state:

```rust
if attempt.class_id != state_params.base_class_id() {
    let epoch_index = ctx.daa_score / state_params.epoch_length();
    let budgets = state
        .epoch_budgets()
        .filter(|b| b.epoch_index == epoch_index)
        .ok_or(PalwAdmissionV2Error::EpochBudgetUnspecified(attempt.class_id))?;
```
(palw_admission_v2.rs:275-280)

`PalwChainStateV2` holds exactly one `PalwEpochBudgetsV2 { epoch_index, budget_blocks }`, and the
only writer is `ensure_epoch_budgets`, which runs INSIDE the transition at step 3b and keys on
`ctx.daa_score / epoch_length` (palw_state_v2.rs:3908-3912). So after block P's transition the
stored record carries `P.daa_score / epoch_length`, and for the first block C whose `daa_score`
lands in the next epoch, `filter` yields `None`.

The block's OWN work is admitted before the transition ever runs and is never re-admitted
afterwards: the UTXO walk calls `let attempt = match self.palw_v2_check_attempt_admission(&header,
state, state_params, &point)` with `state` = the parent state (processor.rs:1331-1339), and on
`Err` sets `StatusDisqualifiedFromChain`; step 4 of `apply_palw_transition_v4` then calls
`apply_attempt` directly (palw_state_v2.rs:3372-3376), and `apply_attempt` checks only duplicate-
claim, bond and class status — never pwu, lottery, budget or exposure. So the parent-state verdict
is the only verdict, and at a boundary it is `EpochBudgetUnspecified` for every non-base class.
The base class is exempt by the `if` on line 275, which is why the defect is invisible from the
only class that keeps producing — the same blind spot
`a_class_that_activates_mid_epoch_is_not_budgeted_until_the_next_boundary` documents one step
away, for a different cause.

The same call is what `palw_v2_merged_works` uses (processor.rs:4941), so the merged copies of
such blocks are pre-skipped too and never reach the transition's post-`ensure_epoch_budgets` re-
check.

**Failure scenario.** Shipped RC bundle, `EPOCH_LENGTH = 1_000`. The chain sits at DAA 999 with the floor class and one
registered non-floor class Q (this is testnet-11's live shape: floor + Qwen2.5-A16 + Qwen3.6). The
next candidate chain block C is a Q attempt with `daa_score = 1000`.
`palw_v2_check_attempt_admission` runs against the parent state, whose `epoch_budgets.epoch_index
== 0`, while `ctx.daa_score / 1000 == 1` → `EpochBudgetUnspecified(Q)` → `info!("Block {} is
disqualified from virtual chain (PALW admission)")` and `StatusDisqualifiedFromChain`. Q's
producer loses the block's chain candidacy and its claim (and is then paid for it anyway with no
claim — finding 1's mechanism). Deterministic on every node; the disqualified block does not
change the stored epoch, so the next Q candidate at DAA 1001 fails identically, and so on until a
block whose own work is a floor attempt, a receipt spend, or nothing crosses the boundary and lets
`ensure_epoch_budgets` install the new table.

Bounded in the normal configuration because the liveness floor is exempt and is the majority
producer. The unbounded case is real but conditional: if at an epoch boundary no floor producer
and no receipt-lane producer is online — a plausible operational state, since the floor producer
being mandatory is an operational convention, not a consensus rule — every candidate chain block
is a non-floor attempt and every one is disqualified for the same reason. DAA keeps advancing
through non-chain blocks, so the epoch never rewinds and the condition never clears on its own:
the selected chain stops at the boundary until someone starts a floor or receipt producer. That is
exactly the deadlock shape the floor exemption's own comment says it exists to make
unrepresentable ("the floor can always produce, so DAA always advances") — the exemption works,
the budget LOOKUP does not.

### S-17 `[medium]` The domain is a consensus value with no pin and no fingerprint: the one "frozen" golden vector pins the abandoned name-only variant, and the footgun that produced R-8 is still exported as the pipeline's name
`consensus/core/src/palw_attempt_v2.rs:667`  ·  verifier lenses: medium, low

**Mechanism.** This changeset split the derivation in two. `palw_network_domain_v2_for`
(`palw_attempt_v2.rs:155-166`) is the consensus value; its Some-branch is three lines added by
this changeset:

```rust
pub fn palw_network_domain_v2_for(network_id: &[u8], genesis: Option<Hash64>) -> Hash64 {
    let mut state = keyed(PALW_ATTEMPT_V2_DOMAIN_NETWORK);
    state.update(&(network_id.len() as u64).to_le_bytes());
    state.update(network_id);
    if let Some(genesis) = genesis {
        state.update(genesis.as_byte_slice());
    }
    finish(state)
}
```

The only golden pin in the tree is on the OTHER branch.
`the_network_domain_separates_networks_and_stays_put` (`:667-685`) asserts

```rust
let t11 = palw_network_domain_v2(b"testnet-11");   // = ..._for(.., None)
assert_eq!(format!("{t11}"), "3a9be06a5e9ca299…", "the testnet-11 domain is frozen");
```

and its doc claims the guarantee: "the challenge's network identity is a consensus value, so a
build whose derivation moves is a build that cannot follow the network it claims". After M2-18 no
consensus path calls that function, so the pin freezes a value nothing uses. `grep -rn
'"[0-9a-f]\{128\}"'` over consensus/ and kaspad/ returns 44 literals and none of them is a
function of the genesis-bound domain; `grep palw_network_domain_v2_for | grep -i
'assert|frozen|pin'` returns nothing.

Nothing else covers it either. `consensus_params_id` (`config/params.rs:1070-1123`) destructures
`Params` field by field and hashes `net` and `genesis.hash` — the two INPUTS — but a fingerprint
over params cannot cover a derivation that lives in code. `palw_ruleset_id_v2` excludes it by
design: `palw_mode_v2.rs:933-935` says "Everything that decides consensus is inside; network
identity is not (the challenge's `network_domain` carries it)". And every test in the tree
computes signer and verifier through the same function, so the whole suite is invariant under a
change to that Some-branch.

The loaded footgun is still in the API. `palw_mode_v2.rs:126` re-exports the name-only variant
under a doc comment that records this exact bug happening once before — "TWICE under two different
domain keys … Two derivations of a consensus identity are two identities: a correctly-formed
attempt would have been refused by the validator that computed the domain the other way" — and
then vouches for it: "this path is that function under the name the pipeline calls it by." That
sentence is now false; the pipeline calls `palw_network_domain_v2_for`. A new site reaching for
the obvious name gets the wrong domain and a comment saying it is right.

**Failure scenario.** Concrete: a later commit edits the Some-branch at `palw_attempt_v2.rs:159-161` — reorders the
update past `network_id`, length-prefixes the genesis, or switches `as_byte_slice()` to
`as_bytes()`. `cargo test` is fully green: `the_network_domain_separates_networks_and_stays_put`
passes (it exercises the None path), `shipped_presets_have_pinned_fingerprints` (`params.rs:5277`)
passes (the pin is `Params::from(net).consensus_params_id()`, which does not hash the derivation),
and every PALW pipeline test passes (both halves move together). Node A on the old build and node
B on the new build then have byte-identical `consensus_params_id`, so `flow_context.rs:1405` finds
them equal, takes neither the `WrongConsensusParams` refusal nor the schedule-mismatch warning,
and keeps the peer with no diagnostic at all. From that moment A's producer signs every attempt
under one domain and B's `check_palw_carriage_stateless` (`pre_ghostdag_validation.rs:156-157`)
recomputes the other, so `validate_stateless_v2` (`palw_attempt_v2.rs:428-433`) returns
`ChallengeMismatch` and B rejects 100% of A's algo-6 headers with
`RuleError::BadPalwCarriageAdmission`, and A rejects B's. Two chains on one genesis, with the
handshake, the preset fingerprints, and the domain's own "frozen" pin all reporting agreement —
R-8's exact shape (two halves, no test comparing either to a fixed value), one level up.
Equivalently, a newly added verifier that reaches for `palw_mode_v2::palw_network_domain_v2` — the
name whose doc says it is what the pipeline calls — is on the wrong domain and no test in the tree
fails.

### S-18 `[medium]` The R-8 repair broke palw-jobs-export on its own default network string, and still derives the domain's network bytes from the raw CLI string instead of the parsed NetworkId
`tools/palw-jobs-export/src/main.rs:125`  ·  verifier lenses: low, low

**Recorded as fixed.** re-audit R-8 remediation: "tools/palw-jobs-export — which re-derives job anchors offline for the
explorer — now resolves the genesis from the network's own shipped params"

**Mechanism.** Commit f0715718 (the re-audit's own fix) added:

```rust
// main.rs:125-128
let genesis = kaspa_consensus_core::network::NetworkId::from_str(&network)
    .map(|net| kaspa_consensus_core::config::params::Params::from(net).genesis.hash)
    .unwrap_or_else(|e| die(format!("--network {network}: {e}")));
let domain = palw_network_domain_v2_for(network.as_bytes(), Some(genesis));
```
while the default is `let mut network = "misaka-testnet-11".to_string();` (`main.rs:102`).
`NetworkId::from_str` (`consensus/core/src/network.rs:357-371`) splits on `'-'` and feeds the
first token to `NetworkType::from_str` (`network.rs:133-141`), which accepts only
`mainnet|testnet|simnet|devnet`. `"misaka"` is none of them, so parsing the prefixed form errors.
The repo's own prefixed form has a dedicated parser, `NetworkId::from_prefixed`
(`network.rs:319-324`), which this line does not use. Second half: the domain's first argument is
`network.as_bytes()`, the raw string, while every verifier uses `params.net.to_string()`
(`processor.rs:711`, `header_processor/processor.rs:229`) and the producer uses
`config.params.net.to_string()` (`daemon.rs:1269`).

**Failure scenario.** (a) Run the tool as shipped — `palw-jobs-export --blocks dump.json --retention <dir> --out
jobs.json`, no `--network` — and it now exits with `--network misaka-testnet-11:
<NetworkTypeError>` instead of producing output. Before f0715718 the same invocation ran to
completion. The same happens for an explicit `--network misaka-testnet-11`, which is the prefixed
spelling the repo itself uses for datadirs (`daemon.rs`: `app_dir.join(network.to_prefixed())`).
(b) Run it as `--network Testnet-11`: `NetworkType::from_str` lowercases, so parsing SUCCEEDS and
the genesis is right, but `network.as_bytes()` is `b"Testnet-11"` while the chain's domain is over
`b"testnet-11"`, so every job anchor and therefore every prompt the explorer displays is wrong
while looking plausible — the exact outcome the comment three lines above claims to have fixed.

### S-19 `[low]` `wallet utxo list` counts locked bond collateral in the mature balance it reports, so the diagnostic command contradicts the spender
`misaka-cli/src/wallet.rs:279`  ·  verifier lenses: low, low

**Recorded as fixed.** M1-3, recorded 'fixed' as 'both spenders skip them' (docs/palw-mainnet-audit-2026-08-28.md:108) —
accurate as far as it goes; the remediation wired the flag into the two spenders and left the
reporting command reading the same struct without it.

**Mechanism.** `page_all` computes `bonded` for every entry (`wallet.rs:222`) and `Funding` carries it
(`wallet.rs:49`), but the summary loop never reads it (`wallet.rs:279-287`):

```rust
for u in &utxos {
    if u.mature {
        mature_n += 1;
        mature_sum += u.amount;
    } else {
        imm_n += 1;
        imm_sum += u.amount;
    }
}
```

A locked bond output is non-coinbase, so `is_spendable_settled` returns `true` (`kaspa-pq-
validator-core/src/lib.rs:1813-1815`) and it lands in `mature_sum`. Neither the JSON nor the Human
branch (`wallet.rs:288-317`) emits a bonded count or amount anywhere. Both spenders, by contrast,
drop it: `filter(|u| u.mature && !u.bonded)` at `wallet.rs:344` and `wallet.rs:466`. The only
place the flag is computed is therefore the only place it is discarded — and it is the command
`docs/misaka-miner-address-check-ja.md:7` and `docs/palw-stage0-shadow-drill-runbook.md:62` tell
operators to use to check a balance.

**Failure scenario.** A validator's funding address holds one output: an Active 20,000 MSK StakeBond's locked
collateral, and nothing else. `misaka wallet utxo list --key-file <seed>` prints `UTXOs total : 1`
/ `mature : 1 (20000.00000000 MSK)` / `immature : 0`. The operator then runs `misaka wallet send
--to … --amount 10000000000 --yes` and gets `insufficient mature funds at <addr>: have 0.00000000
MSK across 0 UTXO(s) (cap 20), need … MSK` (`wallet.rs:481-492`, reached because
`selected.is_empty()`). Two shipped commands against the same node and the same address report
20,000 MSK and 0 MSK for the same question, with no line anywhere explaining that the gap is a
bond — so the operator's first reading is that either the balance or the funds are gone.
Consequence is bounded to a false diagnosis, not to money.

### S-20 `[low]` palw-jobs-export cannot run with its own default network, and silently derives the wrong domain for any accepted spelling that is not exactly `net.to_string()`
`tools/palw-jobs-export/src/main.rs:125`  ·  verifier lenses: low, low

**Mechanism.** The R-8 repair added a parse of the network name to recover the genesis, but left the default and
left the domain keyed off the RAW argument string rather than the parsed id:

```rust
let mut network = "misaka-testnet-11".to_string();          // :102
…
let genesis = kaspa_consensus_core::network::NetworkId::from_str(&network)
    .map(|net| kaspa_consensus_core::config::params::Params::from(net).genesis.hash)
    .unwrap_or_else(|e| die(format!("--network {network}: {e}")));   // :125-127
let domain = palw_network_domain_v2_for(network.as_bytes(), Some(genesis));  // :128
```

`NetworkId::from_str` (`consensus/core/src/network.rs:357-371`) does `network_name.split('-')` and
feeds the FIRST token to `NetworkType::from_str`, which accepts only
`mainnet|testnet|simnet|devnet` (`:133-141`). For `"misaka-testnet-11"` the first token is
`"misaka"` → `NetworkTypeError::InvalidNetworkType` → `die`. Separately, `NetworkType::from_str`
lowercases its input while the domain is computed over `network.as_bytes()` unchanged, so the
parse and the domain disagree on case. Consensus always uses `params.net.to_string()`, i.e.
exactly `b"testnet-11"` (`Display` at `network.rs:374-377`).

**Failure scenario.** (a) `palw-jobs-export --blocks dump.json --retention ~/palw-retention --out jobs.json` — the
invocation the tool's own default is written for — exits 1 with `--network misaka-testnet-11:
Invalid network type: misaka`, and the explorer's job export produces nothing. `misaka-testnet-11`
is the name this repo uses for the network everywhere else (`NetworkId::to_prefixed`,
`network.rs:315`), so it is the natural value to pass explicitly too, and it fails the same way.
(b) `--network Testnet-11` parses (the lowercase in `NetworkType::from_str`), yields the correct
genesis, and then computes `palw_network_domain_v2_for(b"Testnet-11", Some(genesis))` — a domain
no verifier uses. Every `palw_job_anchor_v1` at `:156` is then wrong, so every `prompt_ids` row
the tool writes is a plausible-looking wrong prompt for the block it names, with no error and no
mismatch counter incremented. That is verbatim the outcome the fix's own comment at `:120-124`
says it exists to prevent.

---

## Partially refuted — one lens held, one broke

Kept because the half that held is specific, and because a refutation that turns on one reading is not a closure.

- `[critical]` **M2-1's deciding half is untouched: 4 unauthenticated ~70-byte payloads still refuse the honest producer's material network-wide, and three fixes in this changeset composed to delete every recovery path** — `protocol/flows/src/palw_gossip.rs:276`
  - held: NOT REFUTED as a defect — the mechanism, the attacker capability and the harm are all real at HEAD
— but the finding is stated in a stronger form than the code supports, and the severity is one
notch high.

WHAT I CONFIRMED IN CODE (all at 65674f89):

1. The relay budget is still charged before anything is known about the sender or the bytes.
`protocol/flows/src/palw_gossip.rs:268-280`: `if *count >= PALW_MATERIALS_PER_CLAIM && !solicited
{ return PalwGossipAdmit::Duplicate; }`, with `PALW_MATERIALS_PER_CLAIM = 4` (:50) and `solicited
= self.is_solicited(claim)` (:307), which is true only for
  - refuted: The finding's transport mechanism is accurate; its consequence is not.

Confirmed mechanically:
(a) protocol/flows/src/palw_gossip.rs:264-280 — the per-claim budget is still charged before
anything is known about sender or bytes; the M2-1 fix only added `&& !solicited`, and
`is_solicited` (:255) is true only for a claim THIS node has an open pull on, never true at push
time (the claim is not yet bound, no seat has a duty).
(b) kaspad/src/palw_producer.rs:576 pushes the material before `submit_rpc_block` (:583), and
`rebroadcast_retained`'s loop body is now `let _ = claim;` (palw_producer.rs:31
- `[high]` **Junk in the pool is re-decoded and re-verified on every 2-second tick for the full 300-DAA half-window — a one-time 64 MiB upload buys ~18,000 re-verifications per claim** — `kaspad/src/palw_panel.rs:1786`
  - held: NOT REFUTED. Every load-bearing element of the mechanism is present in the code at HEAD
(65674f89), and I could not find a guard that breaks it.

**The loop is real and unmemoised.**
- `kaspad/src/palw_panel.rs:1296` — `if !self.tick(std::time::Duration::from_secs(2)).await` — the
2 s tick.
- `:1772` — `if answered.contains(&duty.claim_id) || current_daa > duty.receipt_deadline {
continue; }` — the only skip.
- `:1786` — `for bytes in materials.get(&duty.claim_id).map(|v| v.as_slice()).unwrap_or(&[])` —
the pool is re-scanned from the top, in FIFO order, every tick.
- `:1807` — `job_anchor_for
  - refuted: MECHANISM CONFIRMED, CONSEQUENCE REFUTED. The finding holds only in a weaker form.

What is true (verified at the cited lines):
- kaspad/src/palw_panel.rs:1772 gates the duty only on `answered.contains(&duty.claim_id) ||
current_daa > duty.receipt_deadline`; `answered.insert(duty.claim_id)` is written only at :1903,
after a verdict is signed. `answered` is never cleared (grep: only :1247 decl, :1772 read, :1903
write).
- The `Unavailable` fallback fires only at `current_daa >= duty.bound_daa.saturating_add(window /
2)` (:1874-1879), i.e. 300 DAA past bind.
- `materials.retain(|claim, _| live.c
- `[high]` **The EVM sidecar is still the import that runs AFTER the destructive utxoset clear, on the network where EVM is genesis-active** — `protocol/flows/src/ibd/flow.rs:2205`
  - held: MECHANISM HOLDS — every cited line checks out at HEAD 65674f89.

CONFIRMED:
1. Ordering: protocol/flows/src/ibd/flow.rs:2193 `consensus.async_clear_pruning_utxo_set().await;`
then :2194 `self.sync_pruning_point_utxoset(...).await?;` then :2205-2208 `if pruning_point !=
self.ctx.config.genesis.hash { // The overlay and PALW sidecars landed above, before the clear.
self.sync_pruning_point_evm_state(consensus, pruning_point).await?; }`. There are three sidecars;
two were moved to :2172-2190, the EVM one was not.
2. Legitimate post-destroy failure paths exist: flow.rs:2324-2329 `if !msg.found { i
  - refuted: REFUTED as stated. The mechanism is real (the EVM sidecar genuinely is the one import left after
the clear, and `evm_active` genuinely is true on testnet-11 — params.rs:2507 inherited via
`..TESTNET_PARAMS` at params.rs:2623), but the CONSEQUENCE is misattributed: at every one of the
three entries to `sync_new_utxo_set` the node has already crossed a commit barrier and is already
in the non-functional transitional state. The clear destroys nothing that was still usable.

1. The finding's key sub-claim is factually wrong. It says `clear_pruning_utxo_set`'s comment
("under the conditions in whic
- `[high]` **M2-6's only test is tautological: it cannot tell the 5-field registration preimage from the 9-field one, so the class-hijack fix is guarded by nothing** — `consensus/src/pipeline/virtual_processor/tests.rs:2030`
  - held: MECHANISM CONFIRMED, SEVERITY CORRECTED DOWN.

What I verified line by line:

(1) Sole test site. `grep -rn palw_class_registration_message_v2` returns exactly four code sites:
the definition at consensus/core/src/palw_state_v2.rs:1461, the verifier at
consensus/src/pipeline/virtual_processor/processor.rs:4599, the node signer at
kaspad/src/palw_panel.rs:716, and the single test call at
consensus/src/pipeline/virtual_processor/tests.rs:2030 (import at :1982). Nothing under testing/
references ClassRegistered; consensus/core/tests/palw_class_daa_epoch_table.rs builds
ClassRegistered objects but
  - refuted: HOLDS AS A MECHANISM, BUT NOT AT THE CLAIMED CONSEQUENCE OR SEVERITY — refuted as stated.

What I confirmed (the finder's facts are accurate):
- The fix is present: `consensus/core/src/palw_state_v2.rs:1479-1483` mixes `artifact_root`,
`slash_value_per_pwu`, `initial_target`, `pwu_rule`, `canonical` into the keyed preimage.
- The grep is right: 4 code sites only (def `palw_state_v2.rs:1461`, verifier `processor.rs:4599`,
signer `palw_panel.rs:715`, test `tests.rs:2030`). No golden vector, no KAT, no reflection test.
`PALW_V2_SIGNATURE_CONTEXTS` (`palw_mode_v2.rs:86-96`) commits only to context
- `[high]` **R-1's handshake gate — the branch that converts a refusal into a warning — is executed by no test; the only test of that gate exercises a non-fence field and therefore only the pre-existing refuse path** — `protocol/flows/src/flow_context.rs:1407`
  - held: HOLDS as a mechanism; severity overstated.

CONFIRMED, point by point:
1. The branch is real and new. protocol/flows/src/flow_context.rs:1404-1424 contains exactly the
quoted code; `git diff 8e982b7e..65674f89 -- protocol/flows/src/flow_context.rs` shows it replaced
an unconditional `return Err(ProtocolError::WrongConsensusParams(..))`.
2. The only integration test of the gate takes the OLD path.
testing/integration/src/ibd_participation_tests.rs:180
`peers_running_different_consensus_rules_do_not_connect` diverges the peers by
`timestamp_deviation_tolerance` (line 190-191). That field IS hash
  - refuted: The coverage claim is factually TRUE but holds only in a much weaker form than "high", and its
headline attribution is wrong.

WHAT SURVIVES. Every coverage claim checks out. flow_context.rs:255-277 has exactly two tests
(rpc_priority_is_high_only_for_attestation_shards, low_priority_rpc_broadcasts_are_throttled),
neither related. The four params tests at 5148/5177/5227/5258 call consensus_identity_id()
directly and never build a Version. ibd_participation_tests.rs:180 diverges on
timestamp_deviation_tolerance, which for_each_fence binds `timestamp_deviation_tolerance: _`
(params.rs:874) — a r
- `[low]` **The launch runbook still tells third-party miners the domain is `H(network_id)`, and no RPC serves the value that replaced it** — `docs/palw-rc-testnet11-launch-runbook.md:372`
  - held: Holds as stated; every sub-claim verified in code. (1) docs/palw-rc-testnet11-launch-
runbook.md:372 says verbatim "(`network_domain` is `H(network_id)`, derivable from what the node
already publishes)", and it is the ONLY line in that runbook mentioning a domain — ADR-0042 (:165,
:180, :477) uses network_domain as a primitive and never defines its preimage, so this is indeed
the only stated derivation for an external implementer. (2) The value is no longer the name alone:
consensus/core/src/palw_attempt_v2.rs:155-163 mixes a length prefix, the id, and the genesis under
a keyed BLAKE2b, and eve
  - refuted: VERIFIED (the residual, real part): `docs/palw-rc-testnet11-launch-runbook.md:372` still reads
"(`network_domain` is `H(network_id)`, derivable from what the node already publishes)", and `git
diff --stat 8e982b7e..HEAD -- docs/palw-rc-testnet11-launch-runbook.md` returns empty — untouched
by the 34-fix remediation, by f0715718, and by the R-8 fix. The live value is
`palw_network_domain_v2_for` = keyed-BLAKE2b(len_le ‖ network_id ‖ genesis)
(`consensus/core/src/palw_attempt_v2.rs:155-166`), and every consensus verifier passes
`Some(genesis)`: `pre_ghostdag_validation.rs:157`, `processor.rs:437
---

## S-21 `[high]` The M2-10 producer gate checks a path that never exists, so a correctly-configured producer panics at startup

`kaspad/src/daemon.rs:1208`

Found by the coverage critic, in the seam between two lanes that each assumed the other owned
`daemon.rs`; confirmed here against the running fleet rather than by reading alone.

**Mechanism.** The gate refuses `--palw-produce` on a ConsensusV2 network unless `--palw-fee-outpoint`
is given *or* a persisted rolling outpoint exists. It builds that second check as:

```rust
!std::path::Path::new(&args.appdir.clone().unwrap_or_default())
    .join(config.params.net.to_string())
    .join("palw-panel")
    .join("palw-fee-outpoint")
    .exists()
```

The panel's real state directory is `app_dir.join(network.to_prefixed()).join("palw-panel")`
(`daemon.rs:1426`), and `to_prefixed()` is `format!("misaka-{self}")` (`network.rs:315`). Two
independent divergences: the gate omits the `misaka-` prefix, and `args.appdir` defaults to `None`
(`args.rs:354`) so `unwrap_or_default()` yields `""` — a *relative* path off the process CWD rather
than the resolved app dir from `get_app_dir_from_args`.

**Confirmed on the live fleet.** On the testnet-11 producer host:

```
/root/.t11/testnet-11/palw-panel          -> No such file or directory   (what the gate checks)
/root/.t11/misaka-testnet-11/palw-panel   -> exists                       (what the panel uses)
/root/.t11/misaka-testnet-11/palw-panel/palw-fee-outpoint  -> 130 bytes, 2026-08-28
```

**Failure scenario.** A producer that has been running on the rolling outpoint it persisted is
restarted without `--palw-fee-outpoint`. The gate's `.exists()` is false — it is always false — and
the branch is a `panic!`, so the node dies at startup on exactly the configuration the gate's own
comment says is permitted ("or the rolling outpoint it persists from one"). The fleet's own units
pass the flag explicitly and are therefore unaffected; a third-party producer following the
documented recipe is not. This is on the build testnet-11 would be redeployed from.

---

## What this sweep still did not cover

Stated because the previous pass's most valuable sentence was the one admitting its own limits.

I ran the one suite the sweep left unrun (`cargo test -p kaspa-consensus-core --lib` — 1347 passed, 0 failed), and found one concrete defect in a file a lane claimed to read. Here are the gaps.

## 1. Files in the changeset that no lane's surface_summary accounts for

**`consensus/core/src/palw_qwen36_profile.rs` (129 lines) — read by zero lanes.** This is the largest unowned file and it is consensus-critical: it lands the M2-9 fix, whose own comment states the old stripper "deleted `attn_o.weight` from every layer" of a full-attention-only member, so the projected graph — and therefore `shape_profile_id()` — necessarily differs at HEAD for `QWEN3_CODER_30B_A3B`; the new test at `palw_qwen36_profile.rs:751` pins only the **hybrid** id (`ec7bbcb…`) and deliberately does not pin the moe id, so the Coder/Huihui class registered post-genesis on the live testnet-11 chain has no regression guard that its id survived this edit. The same file also adds a brand-new startup refusal (`ProfileNotCanonical` at `palw_qwen36_profile.rs:814`) that no lane evaluated.

**`kaspad/src/validator_service.rs` (68 lines) — read by zero lanes.** test-integrity only *mapped* it to a crate and explicitly noted "none [of the 45 kaspad tests] touches … the R-6 rebuild path"; so the M1-4 mode gate and the R-6 change that makes `AllowRebroadcast` re-sign with a **freshly hedged ML-DSA-87 signature** and rewrite the durable fingerprint (`validator_service.rs:1494-1579`) is a slashing-adjacent path with no reader and no test.

**`consensus/core/src/palw_mode_v2.rs` (74 lines) — only glancingly.** M2-court read `PalwCourtParamsV2` and M2-transport read line 50, but nobody owned `PALW_V2_SIGNATURE_CONTEXTS` going from 2 to 8 entries (`palw_mode_v2.rs:88-96`), which feeds `palw_v2_signature_contexts_root()` and therefore **moves the ruleset id**.

**`consensus/core/src/palw_fp_devnet_v3.rs`** (`WINDOW_COURT` 2400→3000, line 49) and **`consensus/core/src/palw_court_v2.rs`** (`PALW_COURT_V2_ALL_DOMAINS` 3→4, line 82) were read as "constants" by M2-court/M2-state, but no lane asked the consequent question — whether the shipped ConsensusV2 bundle's identity moved and whether live testnet-11 state still validates against it.

**`kaspa-pq-validator/src/main.rs`** — R4R5 read lines 1650-1810 for the single-outpoint exclusion and test-integrity ran its 11 tests, but neither judged the M1-5 hunk that sits inside that very range (`main.rs:1714`).

## 2. The claim still verified by reading alone — the next R-8

**M1-5, at `kaspa-pq-validator/src/main.rs:1714.`** The fix replaces `starts_with("Transaction") && ends_with("not found")` with `msg.contains(&txid.to_string()) && msg.contains("not found")`, justified by the assertion that the borsh wRPC client renders the server error through `RpcCall`'s `{0:?}`. Its new test (`main.rs:2205-2215`) hand-builds `format!("RPC response error Text({server:?})")` from that same assumption — it never constructs a real `RpcError`, and the actual string comes from the external `workflow-rpc` `ResponseError` Display, which nothing in this tree exercises (the only in-repo construction is `rpc/macros/src/wrpc/client.rs:74`, `RpcError::RpcSubsystem(e.to_string())`). If the real rendering differs in any way, `MempoolStatus::Gone` stays unreachable exactly as before, `inflight_spent` grows one entry per epoch forever, and M1-5 is recorded fixed while being unfixed — the identical shape to R-8.

Runner-up: **M1-4/R-6** in `validator_service.rs`, which has neither a reader nor a test.

## 3. Modalities not run at all

- **No node was ever booted.** This is the costly one: it would have caught the defect below immediately.
- **No wire-compat run against an old binary** — proto fields 13/14 were reasoned about but never exchanged; `kaspa-p2p-lib` has no `VersionMessage` round-trip test at all, so the "old peer sends empty → refuse" truth table at `protocol/flows/src/flow_context.rs:1404-1424` is argument, not measurement.
- **No multi-node or reorg simulation** — it would most likely have exercised the court role-split and the pending-share table, the two places the sweep reported criticals from reading.
- **`cargo test -p kaspa-consensus-core` was never run by the sweep**, despite that crate holding 7 of 29 changed files and most of the changeset's lines. I ran it: **1347 passed / 0 failed**. There is no second R-8 there — a real result, and it refutes my own leading hypothesis.
- **The entire M2-1/M2-2 transport remediation shipped with zero tests**, as the transport lane itself recorded.

## 4. A surface where two lanes each assumed the other covered it — and the defect in it

**`kaspad/src/daemon.rs:1196-1219`, the M2-10 startup gate.** The ibd-p2p lane listed "the new M2-10 gate + `get_app_dir_from_args`" and R1R2/R8 listed `daemon.rs` for genesis and fingerprint only; nobody compared the gate against the resolver sitting 900 lines above it. The gate builds its existence check as `args.appdir.clone().unwrap_or_default()` joined with `config.params.net.to_string()`, but the panel's real state directory is `app_dir.join(network.to_prefixed()).join("palw-panel")` (`daemon.rs:1426`), where `to_prefixed()` is `format!("misaka-{}", self)` (`consensus/core/src/network.rs:315`) and `app_dir` comes from `get_app_dir_from_args` (`daemon.rs:279-286`). Two independent divergences: the gate omits the `misaka-` prefix, and `appdir` defaults to `None` (`kaspad/src/args.rs:354`), so `unwrap_or_default()` yields `""` and the check resolves to a **relative** `testnet-11/palw-panel/palw-fee-outpoint` off the process CWD. The `.exists()` is therefore always false in the default configuration, and the gate is a hard `panic!` — so a producer that has been running fine on its persisted rolling fee outpoint, restarted without `--palw-fee-outpoint`, crashes at startup on exactly the configuration M2-10's own comment says is permitted ("or the rolling outpoint it persists from one"). **High**: a fix recorded as closed that denies service to correctly-configured producers, on the build testnet-11 is about to be redeployed from.

Two further split-ownership seams: the **ruleset-id/re-mint question** fell between R1R2 (which explicitly scoped out "the ConsensusV2 bundle's internal fences") and M2-court/M2-state (which read the bundle constants but not as identity); and **`virtual_processor/tests.rs`**, where R3 judged the pruning test, R8 read the domain fixtures, and test-integrity ran the suite, but no lane owned whether the class-id and profile tests pin what they claim.

---

## testnet-11: what an update would actually take

Measured against the live network on 2026-08-29, not inferred from the runbooks.

| | |
|---|---|
| fingerprint the fleet runs | `15bab795442ec3efc3a58e02dd9c7a6f3015ff0634bc4a50a7af589338857ad0` |
| fingerprint this build pins | `404f8715d962c9284c957f63031ef8d77fe43bd5c80534dc37d51eb19ad8bf7a` |
| genesis constants | **unchanged** by this changeset — the 347M community allocation and all 41 premine UTXOs survive a re-mint |
| chain position | `sink_daa=2093` |

**It is a re-mint, not a swap.** `/root/deploy-t11.sh` gates on "the candidate must have synced this
chain from nothing and disqualified NOTHING", and refuses otherwise. This build cannot pass that
gate: the transition rules changed (M2-3, M2-5, M2-7, M2-8, M2-11, M2-12, M2-16 per the pin's own
note), so the candidate necessarily disqualifies blocks the live chain accepted. The script is right
to refuse; the honest reading is that the chain has to be discarded and restarted from the same
genesis on the new rules.

**It affects third parties.** testnet-11 has community nodes on it, not just the fleet — inbound
peers seen at the producer include `113.155.23.105`, `133.18.141.168`, `183.176.36.141`,
`207.180.230.3`, `217.178.131.170`, `60.114.127.4`. Every one of them must wipe its datadir and
rejoin, or hit the genesis-mismatch guard. That is an announced flag day, not a rolling upgrade.

**The fleet cannot be wiped from one seat.** Reachability measured from the producer host:

| host | role | reachable |
|---|---|---|
| `169.58.39.220` (ibm) | node0 producer + node1 second seat | yes, directly |
| `160.16.131.119` (A) | node + miner | yes, via ibm as `ubuntu` |
| `169.58.232.113` | node + explorer + DNS seeder + MTP service | yes, via ibm as `root` |
| `5.104.81.23` (C) | node + miner | **no** — publickey denied as both `root` and `ubuntu` |
| `169.58.232.114` | node | **no** — publickey denied |

A partial wipe is worse than none: an un-wiped peer re-supplies the old chain by IBD to every host
that was wiped. The fleet wipe has to stop every host first, and two of them cannot be stopped from
here.

**Two pre-existing operational faults, unrelated to this changeset, that a redeploy would inherit:**

- `misaka-t11-node1` is in an OOM restart loop — restart counter 44, `Failed with result 'oom-kill'`,
  37 minutes of CPU consumed per life. Host load average 8.4.
- Host disk is at **96%** (13 GB free on a 290 GB root, with `.t11` at 4.4 GB and `.t11b` at 3.7 GB).
  A rebuild plus a fresh datadir needs headroom that is not there.
- A node at `169.58.13.16` is answering to the testnet-11 network name on a *different* genesis
  (`d25a80b9…` against the fleet's `c664a224…`), and is retrying the handshake every ~2 seconds. It
  is being refused correctly, but it is a stale deployment somebody should be told about.
